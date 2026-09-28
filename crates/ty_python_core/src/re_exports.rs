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
//!
//! ## Modules with `__all__`
//!
//! If `bar` defines `__all__`, `from bar import *` only binds the names listed in it. Which
//! names those are is ultimately decided during type inference, since `__all__` can be built
//! from other modules' `__all__` or changed in branches that are only known to be dead once
//! the configured Python version and platform are taken into account. This query therefore
//! returns a superset of the names in `__all__`, determined from the syntax of `bar` and of the
//! modules it `*`-imports from. If even that superset can't be determined without type inference,
//! it falls back to every name bound in the global scope of `bar`.

use ruff_db::parsed::parsed_module;

use ruff_python_ast::{
    self as ast,
    name::Name,
    statement_visitor::{self, StatementVisitor},
    visitor::{Visitor, walk_expr, walk_pattern, walk_stmt},
};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{ImportingFile, resolve_module_for_import_from};

use crate::{Db, ProgramFile};

#[salsa::tracked(
    returns(deref),
    cycle_initial=|_, _, _| Box::default(),
    heap_size=ruff_memory_usage::heap_size)
]
pub(super) fn exported_names(db: &dyn Db, file: ProgramFile<'_>) -> Box<[Name]> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let mut finder = ExportFinder::new(db, file);
    finder.visit_body(module.suite());

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
            DunderAll::Present => match syntactic_dunder_all_names(self.db, self.program_file) {
                Some(dunder_all) => self
                    .exports
                    .into_keys()
                    .filter(|name| dunder_all.contains(*name))
                    .cloned()
                    .collect(),
                None => self.exports.into_keys().cloned().collect(),
            },
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias {
            name,
            asname,
            range: _,
            node_index: _,
        } = alias;

        let name = &name.id;
        let asname = asname.as_ref().map(|asname| &asname.id);

        // If the source is a stub, names defined by imports are only exported
        // if they use the explicit `foo as foo` syntax:
        let kind = if self.visiting_stub_file && asname.is_none_or(|asname| asname != name) {
            PossibleExportKind::StubImportWithoutRedundantAlias
        } else {
            PossibleExportKind::Normal
        };

        self.possibly_add_export(asname.unwrap_or(name), kind);
    }

    fn visit_pattern(&mut self, pattern: &'db ast::Pattern) {
        match pattern {
            ast::Pattern::MatchAs(ast::PatternMatchAs {
                pattern,
                name,
                range: _,
                node_index: _,
            }) => {
                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    // Wildcard patterns (`case _:`) do not bind names.
                    // Currently `self.possibly_add_export()` just ignores
                    // all names with leading underscores, but this will not always be the case
                    // (in the future we will want to support modules with `__all__ = ['_']`).
                    if name != "_" {
                        self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                    }
                }
            }
            ast::Pattern::MatchMapping(ast::PatternMatchMapping {
                patterns,
                rest,
                keys: _,
                range: _,
                node_index: _,
            }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.possibly_add_export(&rest.id, PossibleExportKind::Normal);
                }
            }
            ast::Pattern::MatchStar(ast::PatternMatchStar {
                name,
                range: _,
                node_index: _,
            }) => {
                if let Some(name) = name {
                    self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                }
            }
            ast::Pattern::MatchSequence(_)
            | ast::Pattern::MatchOr(_)
            | ast::Pattern::MatchClass(_) => {
                walk_pattern(self, pattern);
            }
            ast::Pattern::MatchSingleton(_) | ast::Pattern::MatchValue(_) => {}
        }
    }

    fn visit_stmt(&mut self, stmt: &'db ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                decorator_list,
                arguments,
                type_params: _, // We don't want to visit the type params of the class
                body: _,        // We don't want to visit the body of the class
                range: _,
                node_index: _,
            }) => {
                self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = arguments {
                    self.visit_arguments(arguments);
                }
            }

            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                decorator_list,
                parameters,
                returns,
                type_params: _, // We don't want to visit the type params of the function
                body: _,        // We don't want to visit the body of the function
                range: _,
                node_index: _,
                is_async: _,
            }) => {
                self.possibly_add_export(&name.id, PossibleExportKind::Normal);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(parameters);
                if let Some(returns) = returns {
                    self.visit_expr(returns);
                }
            }

            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value,
                annotation,
                simple: _,
                range: _,
                node_index: _,
            }) => {
                if value.is_some() || self.visiting_stub_file {
                    self.visit_expr(target);
                }
                self.visit_expr(annotation);
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }

            ast::Stmt::TypeAlias(ast::StmtTypeAlias {
                name,
                type_params: _,
                value: _,
                range: _,
                node_index: _,
            }) => {
                self.visit_expr(name);
                // Neither walrus expressions nor statements cannot appear in type aliases;
                // no need to recursively visit the `value` or `type_params`
            }

            ast::Stmt::ImportFrom(node) => {
                let mut found_star = false;
                for name in &node.names {
                    if &name.name.id == "*" {
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

            ast::Stmt::Import(_)
            | ast::Stmt::AugAssign(_)
            | ast::Stmt::While(_)
            | ast::Stmt::If(_)
            | ast::Stmt::With(_)
            | ast::Stmt::Assert(_)
            | ast::Stmt::Try(_)
            | ast::Stmt::Expr(_)
            | ast::Stmt::For(_)
            | ast::Stmt::Assign(_)
            | ast::Stmt::Match(_) => walk_stmt(self, stmt),

            ast::Stmt::Global(_)
            | ast::Stmt::Raise(_)
            | ast::Stmt::Return(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_)
            | ast::Stmt::Delete(_)
            | ast::Stmt::Nonlocal(_)
            | ast::Stmt::Pass(_) => {}
        }
    }

    fn visit_expr(&mut self, expr: &'db ast::Expr) {
        match expr {
            ast::Expr::Name(ast::ExprName {
                id,
                ctx,
                range: _,
                node_index: _,
            }) => {
                if ctx.is_store() {
                    self.possibly_add_export(id, PossibleExportKind::Normal);
                }
            }

            ast::Expr::Lambda(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::StringLiteral(_) => {}

            // Walrus definitions "leak" from comprehension scopes into the comprehension's
            // enclosing scope; they thus need special handling
            ast::Expr::SetComp(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::Generator(_)
            | ast::Expr::DictComp(_) => {
                let mut walrus_finder = WalrusFinder {
                    export_finder: self,
                };
                walk_expr(&mut walrus_finder, expr);
            }

            ast::Expr::BoolOp(_)
            | ast::Expr::Named(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::UnaryOp(_)
            | ast::Expr::If(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::Starred(_)
            | ast::Expr::Call(_)
            | ast::Expr::Compare(_)
            | ast::Expr::Yield(_)
            | ast::Expr::YieldFrom(_)
            | ast::Expr::FString(_)
            | ast::Expr::TString(_)
            | ast::Expr::Tuple(_)
            | ast::Expr::List(_)
            | ast::Expr::Slice(_)
            | ast::Expr::IpyEscapeCommand(_)
            | ast::Expr::Dict(_)
            | ast::Expr::Set(_)
            | ast::Expr::Await(_) => walk_expr(self, expr),
        }
    }
}

struct WalrusFinder<'a, 'db> {
    export_finder: &'a mut ExportFinder<'db>,
}

impl<'db> Visitor<'db> for WalrusFinder<'_, 'db> {
    fn visit_expr(&mut self, expr: &'db ast::Expr) {
        match expr {
            // It's important for us to short-circuit here for lambdas specifically,
            // as walruses cannot leak out of the body of a lambda function.
            ast::Expr::Lambda(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::StringLiteral(_)
            | ast::Expr::Name(_) => {}

            ast::Expr::Named(ast::ExprNamed {
                target,
                value: _,
                range: _,
                node_index: _,
            }) => {
                if let ast::Expr::Name(ast::ExprName {
                    id,
                    ctx: ast::ExprContext::Store,
                    range: _,
                    node_index: _,
                }) = &**target
                {
                    self.export_finder
                        .possibly_add_export(id, PossibleExportKind::Normal);
                }
            }

            // We must recurse inside nested comprehensions,
            // as even a walrus inside a comprehension inside a comprehension in the global scope
            // will leak out into the global scope
            ast::Expr::DictComp(_)
            | ast::Expr::SetComp(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::Generator(_)
            | ast::Expr::BoolOp(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::UnaryOp(_)
            | ast::Expr::If(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::Starred(_)
            | ast::Expr::Call(_)
            | ast::Expr::Compare(_)
            | ast::Expr::Yield(_)
            | ast::Expr::YieldFrom(_)
            | ast::Expr::FString(_)
            | ast::Expr::TString(_)
            | ast::Expr::Tuple(_)
            | ast::Expr::List(_)
            | ast::Expr::Slice(_)
            | ast::Expr::IpyEscapeCommand(_)
            | ast::Expr::Dict(_)
            | ast::Expr::Set(_)
            | ast::Expr::Await(_) => walk_expr(self, expr),
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

/// Returns every name that `__all__` can contain in `file`, as far as that can be determined from
/// the syntax of `file` and the modules it `*`-imports from.
///
/// This mirrors the `dunder_all_names` query in `ty_python_semantic`, which decides which names a
/// `*` import binds but needs type inference to do so. Whenever this query returns `Some(names)`,
/// `dunder_all_names` returns a subset of `names`. This query returns `None` whenever
/// `dunder_all_names` might return `None`, in which case a `*` import can bind any name in the
/// global scope of `file`. It also returns `None` if `__all__` is changed in a way that can't be
/// followed without type inference, for example:
///
/// ```python
/// __all__ = ["a"]
/// __all__ += submodule.__all__
/// ```
#[salsa::tracked(
    returns(as_ref),
    cycle_initial=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size
)]
fn syntactic_dunder_all_names(db: &dyn Db, file: ProgramFile<'_>) -> Option<FxHashSet<Name>> {
    let module = parsed_module(db, file.python_file(db)).load(db);

    // `dunder_all_names` returns `None` if it doesn't see `__all__` being defined, and it skips
    // any `if` branch that it can't prove is taken. Only a definition directly in the module's
    // body is certain to be seen.
    if !module.suite().iter().any(is_dunder_all_definition) {
        return None;
    }

    let mut collector = SyntacticDunderAllCollector {
        db,
        file,
        names: FxHashSet::default(),
        unknown: false,
    };
    collector.visit_body(module.suite());
    collector.into_names()
}

/// Collects the names for [`syntactic_dunder_all_names`].
///
/// Unlike `dunder_all_names`, this can't tell which branch of an `if` statement is taken, so it
/// visits all of them and accumulates every name that is ever added to `__all__`. Reassigning or
/// removing from `__all__` therefore never removes a name.
struct SyntacticDunderAllCollector<'db> {
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    names: FxHashSet<Name>,
    /// Set if `__all__` is changed in a way that makes `dunder_all_names` return `None`, or
    /// whose effect can't be determined from the syntax.
    unknown: bool,
}

impl<'db> SyntacticDunderAllCollector<'db> {
    /// Handles `__all__ = [...]` and `__all__ = (...)`.
    fn add_definition(&mut self, value: &ast::Expr) {
        match value {
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => self.add_names(elts),
            _ => self.unknown = true,
        }
    }

    /// Handles `__all__ += ...` and `__all__.extend(...)`.
    fn extend(&mut self, value: &ast::Expr) {
        match value {
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. })
            | ast::Expr::Set(ast::ExprSet { elts, .. }) => self.add_names(elts),
            // This includes `__all__ += submodule.__all__`, which requires knowing which module
            // `submodule` refers to.
            _ => self.unknown = true,
        }
    }

    /// Handles `__all__.extend(...)`, `__all__.append(...)` and `__all__.remove(...)`.
    fn process_call_idiom(&mut self, method: &ast::Identifier, arguments: &ast::Arguments) {
        let argument = if arguments.len() == 1 {
            arguments.find_positional(0)
        } else {
            None
        };
        match (method.as_str(), argument) {
            ("extend", Some(argument)) => self.extend(argument),
            ("append", Some(argument)) => self.add_names(std::slice::from_ref(argument)),
            ("remove", Some(argument)) if argument.is_string_literal_expr() => {}
            _ => self.unknown = true,
        }
    }

    fn add_names(&mut self, elts: &[ast::Expr]) {
        for elt in elts {
            let Some(literal) = elt.as_string_literal_expr() else {
                self.unknown = true;
                return;
            };
            self.names.insert(Name::new(literal.value.to_str()));
        }
    }

    /// Returns the names that `__all__` can contain in the module imported by `import_from`.
    fn names_for_import_from(
        &self,
        import_from: &ast::StmtImportFrom,
    ) -> Option<&'db FxHashSet<Name>> {
        let db = self.db;
        let importing_file =
            ImportingFile::File(self.file.file(db), self.file.resolver_environment(db));
        let module = resolve_module_for_import_from(db, importing_file, import_from)?;
        syntactic_dunder_all_names(
            db,
            ProgramFile::new(db, module.file(db)?, self.file.program(db)),
        )
    }

    fn into_names(mut self) -> Option<FxHashSet<Name>> {
        if self.unknown {
            return None;
        }
        self.names.shrink_to_fit();
        Some(self.names)
    }
}

impl<'db> StatementVisitor<'db> for SyntacticDunderAllCollector<'db> {
    fn visit_stmt(&mut self, stmt: &'db ast::Stmt) {
        if self.unknown {
            return;
        }

        match stmt {
            ast::Stmt::ImportFrom(import_from) => {
                for ast::Alias { name, asname, .. } in &import_from.names {
                    // `from module import *` makes `__all__` invalid if `module` doesn't have a
                    // valid `__all__`, and replaces `__all__` with the names in `module.__all__` if
                    // those include `"__all__"`. `from module import __all__` replaces it too.
                    let is_star = name == "*";
                    let is_dunder_all_import = name == "__all__"
                        && asname.as_ref().is_none_or(|asname| asname == "__all__");
                    if !is_star && !is_dunder_all_import {
                        continue;
                    }
                    let Some(names) = self.names_for_import_from(import_from) else {
                        self.unknown = true;
                        return;
                    };
                    if is_dunder_all_import || names.contains(&Name::new_static("__all__")) {
                        self.names.extend(names.iter().cloned());
                    }
                }
            }

            ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                if let [target] = targets.as_slice()
                    && is_dunder_all(target)
                {
                    self.add_definition(value);
                }
            }

            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => {
                if is_dunder_all(target) {
                    self.add_definition(value);
                }
            }

            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op: ast::Operator::Add,
                value,
                ..
            }) => {
                if is_dunder_all(target) {
                    self.extend(value);
                }
            }

            ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
                if let Some(ast::ExprCall {
                    func, arguments, ..
                }) = value.as_call_expr()
                    && let Some(ast::ExprAttribute { value, attr, .. }) = func.as_attribute_expr()
                    && is_dunder_all(value)
                {
                    self.process_call_idiom(attr, arguments);
                }
            }

            ast::Stmt::If(_)
            | ast::Stmt::For(_)
            | ast::Stmt::While(_)
            | ast::Stmt::With(_)
            | ast::Stmt::Match(_)
            | ast::Stmt::Try(_) => statement_visitor::walk_stmt(self, stmt),

            // `__all__` is only meaningful in the module's global scope.
            ast::Stmt::FunctionDef(_) | ast::Stmt::ClassDef(_) => {}

            ast::Stmt::AugAssign(_)
            | ast::Stmt::AnnAssign(_)
            | ast::Stmt::Delete(_)
            | ast::Stmt::Return(_)
            | ast::Stmt::Raise(_)
            | ast::Stmt::Assert(_)
            | ast::Stmt::Import(_)
            | ast::Stmt::Global(_)
            | ast::Stmt::Nonlocal(_)
            | ast::Stmt::TypeAlias(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_) => {}
        }
    }
}

/// Returns `true` for `__all__ = [...]`, `__all__ = (...)` and their annotated equivalents.
fn is_dunder_all_definition(stmt: &ast::Stmt) -> bool {
    let (target, value) = match stmt {
        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            let [target] = targets.as_slice() else {
                return false;
            };
            (target, &**value)
        }
        ast::Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            value: Some(value),
            ..
        }) => (&**target, &**value),
        _ => return false,
    };
    is_dunder_all(target) && matches!(value, ast::Expr::List(_) | ast::Expr::Tuple(_))
}

fn is_dunder_all(expr: &ast::Expr) -> bool {
    matches!(expr, ast::Expr::Name(ast::ExprName { id, .. }) if id == "__all__")
}
