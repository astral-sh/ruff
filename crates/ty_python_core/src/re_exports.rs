//! A visitor and query to find all global-scope symbols that are exported from a module
//! when a wildcard import is used.
//!
//! For example, if a module `foo` contains `from bar import *`, which symbols from the global
//! scope of `bar` are imported into the global namespace of `foo`?
//!
//! ## Why is this a separate query rather than a part of semantic indexing?
//!
//! This query is called by the [`super::SemanticIndexBuilder`] in order to add the correct
//! [`super::Definition`]s to the semantic index of a module `foo` if `foo` has a
//! `from bar import *` statement in its global namespace. Adding the correct `Definition`s to
//! `foo`'s [`super::SemanticIndex`] requires knowing which symbols are exported from `bar`.
//!
//! If we determined the set of exported names during semantic indexing rather than as a
//! separate query, we would need to complete semantic indexing on `bar` in order to
//! complete analysis of the global namespace of `foo`. Since semantic indexing is somewhat
//! expensive, this would be undesirable. A separate query allows us to avoid this issue.
//!
//! An additional concern is that the recursive nature of this query means that it must be able
//! to handle cycles. We do this using fixpoint iteration; adding fixpoint iteration to the
//! whole [`super::semantic_index()`] query would probably be prohibitively expensive.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, ExprKind, PatternKind};

use ruff_python_ast::{
    Alias, Expr, ExprContext, Identifier, Pattern, Stmt,
    name::Name,
    visitor::{Visitor, walk_expr, walk_pattern, walk_stmt},
};
use rustc_hash::FxHashMap;
use ty_module_resolver::{ImportingFile, resolve_module_for_import_from};

use crate::static_dunder_all::static_dunder_all;
use crate::{Db, ProgramFile};

#[salsa::tracked(
    returns(deref),
    cycle_initial=|_, _, _| Box::default(),
    heap_size=ruff_memory_usage::heap_size)
]
pub(super) fn exported_names(db: &dyn Db, file: ProgramFile<'_>) -> Box<[Name]> {
    if let Some(names) = static_dunder_all(db, file) {
        return names.into();
    }

    let module = parsed_module(db, file.python_file(db)).load(db);
    let mut finder = ExportFinder::new(db, file);
    finder.visit_body(module.root().body());

    let mut exports = finder.resolve_exports();

    // Sort the exports to ensure convergence regardless of hash map
    // or insertion order. See <https://github.com/astral-sh/ty/issues/444>
    exports.sort_unstable();
    exports.into()
}

struct ExportFinder<'db> {
    db: &'db dyn Db,
    program_file: ProgramFile<'db>,
    visiting_stub_file: bool,
    exports: FxHashMap<&'db Name, PossibleExportKind>,
    dunder_all: DunderAll,
}

impl<'db> ExportFinder<'db> {
    fn new(db: &'db dyn Db, file: ProgramFile<'db>) -> Self {
        Self {
            db,
            program_file: file,
            visiting_stub_file: file.file(db).is_stub(db),
            exports: FxHashMap::default(),
            dunder_all: DunderAll::NotPresent,
        }
    }

    fn possibly_add_export(&mut self, export: &'db Name, kind: PossibleExportKind) {
        self.exports.insert(export, kind);

        if export == "__all__" {
            self.dunder_all = DunderAll::Present;
        }
    }

    fn resolve_exports(self) -> Vec<Name> {
        match self.dunder_all {
            DunderAll::NotPresent => self
                .exports
                .into_iter()
                .filter_map(|(name, kind)| {
                    if kind == PossibleExportKind::StubImportWithoutRedundantAlias {
                        return None;
                    }
                    if name.starts_with('_') {
                        return None;
                    }
                    Some(name.clone())
                })
                .collect(),
            DunderAll::Present => self.exports.into_keys().cloned().collect(),
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: Alias<'db>) {
        let name = alias.name();
        let asname = alias.asname();

        let name = name.id();
        let asname = asname.map(Identifier::id);

        // If the source is a stub, names defined by imports are only exported
        // if they use the explicit `foo as foo` syntax:
        let kind = if self.visiting_stub_file && asname.is_none_or(|asname| asname != name) {
            PossibleExportKind::StubImportWithoutRedundantAlias
        } else {
            PossibleExportKind::Normal
        };

        self.possibly_add_export(asname.unwrap_or(name), kind);
    }

    fn visit_pattern(&mut self, pattern: Pattern<'db>) {
        match pattern.classify() {
            PatternKind::MatchAs(pattern_match_as) => {
                let pattern = pattern_match_as.pattern();
                let name = pattern_match_as.name();

                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    // Wildcard patterns (`case _:`) do not bind names.
                    // Currently `self.possibly_add_export()` just ignores
                    // all names with leading underscores, but this will not always be the case
                    // (in the future we will want to support modules with `__all__ = ['_']`).
                    if name != "_" {
                        self.possibly_add_export(name.id(), PossibleExportKind::Normal);
                    }
                }
            }
            PatternKind::MatchMapping(pattern_match_mapping) => {
                let items = pattern_match_mapping.items();
                for (_, pattern) in items.entries() {
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = items.rest() {
                    self.possibly_add_export(rest.id(), PossibleExportKind::Normal);
                }
            }
            PatternKind::MatchStar(pattern_match_star) => {
                let name = pattern_match_star.name();

                if let Some(name) = name {
                    self.possibly_add_export(name.id(), PossibleExportKind::Normal);
                }
            }
            PatternKind::MatchSequence(_)
            | PatternKind::MatchOr(_)
            | PatternKind::MatchClass(_) => {
                walk_pattern(self, pattern);
            }
            PatternKind::MatchSingleton(_) | PatternKind::MatchValue(_) => {}
        }
    }

    fn visit_stmt(&mut self, stmt: Stmt<'db>) {
        match stmt.fields() {
            ast::StmtFields::ClassDef(ast::StmtClassDefFields {
                name,
                decorator_list,
                arguments,
                type_params: _, // We don't want to visit the type params of the class
                body: _,        // We don't want to visit the body of the class
                range: _,
                node: _,
            }) => {
                self.possibly_add_export(name.id(), PossibleExportKind::Normal);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = arguments {
                    self.visit_arguments(arguments);
                }
            }

            ast::StmtFields::FunctionDef(ast::StmtFunctionDefFields {
                name,
                decorator_list,
                parameters,
                returns,
                type_params: _, // We don't want to visit the type params of the function
                body: _,        // We don't want to visit the body of the function
                range: _,
                node: _,
                is_async: _,
            }) => {
                self.possibly_add_export(name.id(), PossibleExportKind::Normal);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(parameters);
                if let Some(returns) = returns {
                    self.visit_expr(returns);
                }
            }

            ast::StmtFields::AnnAssign(ast::StmtAnnAssignFields {
                target,
                value,
                annotation,
                simple: _,
                range: _,
                node: _,
            }) => {
                if value.is_some() || self.visiting_stub_file {
                    self.visit_expr(target);
                }
                self.visit_expr(annotation);
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }

            ast::StmtFields::TypeAlias(ast::StmtTypeAliasFields {
                name,
                type_params: _,
                value: _,
                range: _,
                node: _,
            }) => {
                self.visit_expr(name);
                // Neither walrus expressions nor statements cannot appear in type aliases;
                // no need to recursively visit the `value` or `type_params`
            }

            ast::StmtFields::ImportFrom(ast::StmtImportFromFields { node, .. }) => {
                let mut found_star = false;
                for name in node.names() {
                    if name.name().id() == "*" {
                        if !found_star {
                            found_star = true;
                            let db = self.db;
                            let program_file = self.program_file;
                            let file = program_file.file(db);
                            let resolver_environment = program_file.resolver_environment(db);
                            for export in resolve_module_for_import_from(
                                db,
                                ImportingFile::File(file, resolver_environment),
                                node,
                            )
                            .iter()
                            .flat_map(|module| {
                                module
                                    .file(db)
                                    .map(|file| {
                                        exported_names(
                                            db,
                                            ProgramFile::new(db, file, program_file.program(db)),
                                        )
                                    })
                                    .unwrap_or_default()
                            }) {
                                self.possibly_add_export(export, PossibleExportKind::Normal);
                            }
                        }
                    } else {
                        self.visit_alias(name);
                    }
                }
            }

            ast::StmtFields::Import(_)
            | ast::StmtFields::AugAssign(_)
            | ast::StmtFields::While(_)
            | ast::StmtFields::If(_)
            | ast::StmtFields::With(_)
            | ast::StmtFields::Assert(_)
            | ast::StmtFields::Try(_)
            | ast::StmtFields::Expr(_)
            | ast::StmtFields::For(_)
            | ast::StmtFields::Assign(_)
            | ast::StmtFields::Match(_) => walk_stmt(self, stmt),

            ast::StmtFields::Global(_)
            | ast::StmtFields::Raise(_)
            | ast::StmtFields::Return(_)
            | ast::StmtFields::Break(_)
            | ast::StmtFields::Continue(_)
            | ast::StmtFields::IpyEscapeCommand(_)
            | ast::StmtFields::Delete(_)
            | ast::StmtFields::Nonlocal(_)
            | ast::StmtFields::Pass(_) => {}
        }
    }

    fn visit_expr(&mut self, expr: Expr<'db>) {
        match expr.classify() {
            ExprKind::Name(expr_name) => {
                let id = expr_name.id();
                let ctx = expr_name.ctx();

                if ctx.is_store() {
                    self.possibly_add_export(id, PossibleExportKind::Normal);
                }
            }

            ExprKind::Lambda(_)
            | ExprKind::BooleanLiteral(_)
            | ExprKind::NoneLiteral(_)
            | ExprKind::NumberLiteral(_)
            | ExprKind::BytesLiteral(_)
            | ExprKind::EllipsisLiteral(_)
            | ExprKind::StringLiteral(_) => {}

            // Walrus definitions "leak" from comprehension scopes into the comprehension's
            // enclosing scope; they thus need special handling
            ExprKind::SetComp(_)
            | ExprKind::ListComp(_)
            | ExprKind::Generator(_)
            | ExprKind::DictComp(_) => {
                let mut walrus_finder = WalrusFinder {
                    export_finder: self,
                };
                walk_expr(&mut walrus_finder, expr);
            }

            ExprKind::BoolOp(_)
            | ExprKind::Named(_)
            | ExprKind::BinOp(_)
            | ExprKind::UnaryOp(_)
            | ExprKind::If(_)
            | ExprKind::Attribute(_)
            | ExprKind::Subscript(_)
            | ExprKind::Starred(_)
            | ExprKind::Call(_)
            | ExprKind::Compare(_)
            | ExprKind::Yield(_)
            | ExprKind::YieldFrom(_)
            | ExprKind::FString(_)
            | ExprKind::TString(_)
            | ExprKind::Tuple(_)
            | ExprKind::List(_)
            | ExprKind::Slice(_)
            | ExprKind::IpyEscapeCommand(_)
            | ExprKind::Dict(_)
            | ExprKind::Set(_)
            | ExprKind::Await(_) => walk_expr(self, expr),
        }
    }
}

struct WalrusFinder<'a, 'db> {
    export_finder: &'a mut ExportFinder<'db>,
}

impl<'db> Visitor<'db> for WalrusFinder<'_, 'db> {
    fn visit_expr(&mut self, expr: Expr<'db>) {
        match expr.classify() {
            // It's important for us to short-circuit here for lambdas specifically,
            // as walruses cannot leak out of the body of a lambda function.
            ExprKind::Lambda(_)
            | ExprKind::BooleanLiteral(_)
            | ExprKind::NoneLiteral(_)
            | ExprKind::NumberLiteral(_)
            | ExprKind::BytesLiteral(_)
            | ExprKind::EllipsisLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Name(_) => {}

            ExprKind::Named(expr_named) => {
                let target = expr_named.target();

                if let ast::ExprFields::Name(ast::ExprNameFields {
                    id,
                    ctx: ExprContext::Store,
                    range: _,
                    node: _,
                }) = target.fields()
                {
                    self.export_finder
                        .possibly_add_export(id, PossibleExportKind::Normal);
                }
            }

            // We must recurse inside nested comprehensions,
            // as even a walrus inside a comprehension inside a comprehension in the global scope
            // will leak out into the global scope
            ExprKind::DictComp(_)
            | ExprKind::SetComp(_)
            | ExprKind::ListComp(_)
            | ExprKind::Generator(_)
            | ExprKind::BoolOp(_)
            | ExprKind::BinOp(_)
            | ExprKind::UnaryOp(_)
            | ExprKind::If(_)
            | ExprKind::Attribute(_)
            | ExprKind::Subscript(_)
            | ExprKind::Starred(_)
            | ExprKind::Call(_)
            | ExprKind::Compare(_)
            | ExprKind::Yield(_)
            | ExprKind::YieldFrom(_)
            | ExprKind::FString(_)
            | ExprKind::TString(_)
            | ExprKind::Tuple(_)
            | ExprKind::List(_)
            | ExprKind::Slice(_)
            | ExprKind::IpyEscapeCommand(_)
            | ExprKind::Dict(_)
            | ExprKind::Set(_)
            | ExprKind::Await(_) => walk_expr(self, expr),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PossibleExportKind {
    Normal,
    StubImportWithoutRedundantAlias,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DunderAll {
    NotPresent,
    Present,
}
