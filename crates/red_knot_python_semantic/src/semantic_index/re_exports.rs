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

use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast,
    name::Name,
    visitor::{walk_expr, walk_pattern, walk_stmt, Visitor},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{module_name::ModuleName, resolve_module, Db};

fn exports_cycle_recover(
    _db: &dyn Db,
    _value: &[Name],
    _count: u32,
    _file: File,
) -> salsa::CycleRecoveryAction<Box<[Name]>> {
    salsa::CycleRecoveryAction::Iterate
}

fn exports_cycle_initial(_db: &dyn Db, _file: File) -> Box<[Name]> {
    Box::default()
}

#[salsa::tracked(return_ref, cycle_fn=exports_cycle_recover, cycle_initial=exports_cycle_initial)]
pub(super) fn exported_names(db: &dyn Db, file: File) -> Box<[Name]> {
    let module = parsed_module(db.upcast(), file);
    let mut finder = ExportFinder::new(db, file);
    finder.visit_body(module.suite());
    finder.resolve_exports()
}

struct ExportFinder<'db> {
    db: &'db dyn Db,
    file: File,
    visiting_stub_file: bool,
    exports: FxHashMap<&'db Name, PossibleExportKind>,
    dunder_all: DunderAll<'db>,
}

impl<'db> ExportFinder<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            visiting_stub_file: file.is_stub(db.upcast()),
            exports: FxHashMap::default(),
            dunder_all: DunderAll::default(),
        }
    }

    fn possibly_add_export(&mut self, export: &'db Name, kind: DefinitionKind<'db>) {
        match kind {
            // If the source is a stub, names defined by imports are only exported
            // if they use the explicit `foo as foo` syntax:
            DefinitionKind::Import { alias_name }
                if self.visiting_stub_file && alias_name.is_none_or(|name| name != export) =>
            {
                self.exports
                    .insert(export, PossibleExportKind::StubImportWithoutRedundantAlias);
            }

            _ => {
                self.exports.insert(export, PossibleExportKind::Normal);
            }
        }

        if export == "__all__" && kind != DefinitionKind::AssignOrAnnAssign {
            self.dunder_all = DunderAll::Dynamic;
        }
    }

    fn resolve_exports(self) -> Box<[Name]> {
        match self.dunder_all {
            DunderAll::Static(dunder_all) => self
                .exports
                .into_keys()
                .filter(|key| dunder_all.contains(&***key))
                .cloned()
                .collect(),
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
            DunderAll::Dynamic => self.exports.into_keys().cloned().collect(),
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias { name, asname, .. } = alias;
        self.possibly_add_export(
            &asname.as_ref().unwrap_or(name).id,
            DefinitionKind::Import {
                alias_name: asname.as_deref(),
            },
        );
    }

    fn visit_pattern(&mut self, pattern: &'db ast::Pattern) {
        match pattern {
            ast::Pattern::MatchAs(ast::PatternMatchAs {
                pattern,
                name,
                range: _,
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
                        self.possibly_add_export(&name.id, DefinitionKind::Other);
                    }
                }
            }
            ast::Pattern::MatchMapping(ast::PatternMatchMapping {
                patterns,
                rest,
                keys: _,
                range: _,
            }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.possibly_add_export(&rest.id, DefinitionKind::Other);
                }
            }
            ast::Pattern::MatchStar(ast::PatternMatchStar { name, range: _ }) => {
                if let Some(name) = name {
                    self.possibly_add_export(&name.id, DefinitionKind::Other);
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
            }) => {
                self.possibly_add_export(&name.id, DefinitionKind::Other);
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
                is_async: _,
            }) => {
                self.possibly_add_export(&name.id, DefinitionKind::Other);
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
            }) => {
                self.visit_expr(annotation);

                if let Some(value) = value {
                    self.visit_expr(value);

                    if let ast::Expr::Name(ast::ExprName {
                        id,
                        ctx: ast::ExprContext::Store,
                        range: _,
                    }) = &**target
                    {
                        if id == "__all__" {
                            match &**value {
                                ast::Expr::List(ast::ExprList { elts, .. })
                                | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                    self.dunder_all.extend(elts);
                                }
                                _ => self.dunder_all = DunderAll::Dynamic,
                            }
                        }

                        self.possibly_add_export(id, DefinitionKind::AssignOrAnnAssign);
                    } else {
                        self.visit_expr(target);
                    }
                } else if self.visiting_stub_file {
                    self.visit_expr(target);
                }
            }

            ast::Stmt::TypeAlias(ast::StmtTypeAlias {
                name,
                type_params: _,
                value: _,
                range: _,
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
                            for export in
                                ModuleName::from_import_statement(self.db, self.file, node)
                                    .ok()
                                    .and_then(|module_name| resolve_module(self.db, &module_name))
                                    .iter()
                                    .flat_map(|module| exported_names(self.db, module.file()))
                            {
                                self.possibly_add_export(export, DefinitionKind::Other);
                            }
                        }
                    } else {
                        self.visit_alias(name);
                    }
                }
            }

            ast::Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                range: _,
            }) => {
                if let [ast::Expr::Name(ast::ExprName {
                    id,
                    ctx: ast::ExprContext::Store,
                    range: _,
                })] = &**targets
                {
                    if id == "__all__" {
                        match &**value {
                            ast::Expr::List(ast::ExprList { elts, .. })
                            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                self.dunder_all.extend(elts);
                            }
                            _ => self.dunder_all = DunderAll::Dynamic,
                        }
                    }

                    self.possibly_add_export(id, DefinitionKind::AssignOrAnnAssign);
                } else {
                    for target in targets {
                        self.visit_expr(target);
                    }
                }

                self.visit_expr(value);
            }

            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op: ast::Operator::Add,
                value,
                range: _,
            }) => {
                if let ast::Expr::Name(ast::ExprName {
                    id,
                    ctx: ast::ExprContext::Store,
                    range: _,
                }) = &**target
                {
                    if id == "__all__" {
                        match &**value {
                            ast::Expr::List(ast::ExprList { elts, .. })
                            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                                self.dunder_all.extend(elts);
                            }
                            _ => {
                                self.dunder_all = DunderAll::Dynamic;
                                self.visit_expr(value);
                            }
                        }
                    }
                } else {
                    self.visit_expr(target);
                }

                self.visit_expr(value);
            }

            ast::Stmt::Expr(ast::StmtExpr {
                value: original_value,
                range: _,
            }) => {
                let ast::Expr::Call(ast::ExprCall {
                    func,
                    arguments:
                        ast::Arguments {
                            args,
                            keywords,
                            range: _,
                        },
                    range: _,
                }) = &**original_value
                else {
                    walk_expr(self, original_value);
                    return;
                };

                if !keywords.is_empty() {
                    walk_expr(self, original_value);
                    return;
                }

                let ast::Expr::Attribute(ast::ExprAttribute {
                    value,
                    attr,
                    ctx: ast::ExprContext::Load,
                    range: _,
                }) = &**func
                else {
                    walk_expr(self, original_value);
                    return;
                };

                let ast::Expr::Name(ast::ExprName {
                    id,
                    ctx: _,
                    range: _,
                }) = &**value
                else {
                    walk_expr(self, original_value);
                    return;
                };

                if id != "__all__" {
                    walk_expr(self, original_value);
                    return;
                }

                match &**attr {
                    "extend" => {
                        if let [ast::Expr::List(ast::ExprList { elts, .. })
                        | ast::Expr::Tuple(ast::ExprTuple { elts, .. })] = &**args
                        {
                            self.dunder_all.extend(elts);
                        } else {
                            self.dunder_all = DunderAll::Dynamic;
                            walk_expr(self, original_value);
                        }
                    }
                    "append" => {
                        if let [item] = &**args {
                            self.dunder_all.insert(item);
                        } else {
                            self.dunder_all = DunderAll::Dynamic;
                            walk_expr(self, original_value);
                        }
                    }
                    _ => {
                        walk_expr(self, original_value);
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
            | ast::Stmt::For(_)
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
            ast::Expr::Name(ast::ExprName { id, ctx, range: _ }) => {
                if ctx.is_store() {
                    self.possibly_add_export(id, DefinitionKind::Other);
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
            }) => {
                if let ast::Expr::Name(ast::ExprName {
                    id,
                    ctx: ast::ExprContext::Store,
                    range: _,
                }) = &**target
                {
                    self.export_finder
                        .possibly_add_export(id, DefinitionKind::Other);
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum PossibleExportKind {
    Normal,
    StubImportWithoutRedundantAlias,
}

#[derive(Debug, Default)]
enum DunderAll<'db> {
    #[default]
    NotPresent,
    Static(FxHashSet<&'db str>),
    Dynamic,
}

impl<'db> DunderAll<'db> {
    fn insert(&mut self, item: &'db ast::Expr) {
        match self {
            Self::Dynamic => {}
            _ => match item {
                ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => match self {
                    Self::Static(set) => {
                        set.insert(value.to_str());
                    }
                    Self::NotPresent => {
                        *self = Self::Static(FxHashSet::from_iter([value.to_str()]));
                    }
                    Self::Dynamic => unreachable!(),
                },
                _ => *self = Self::Dynamic,
            },
        }
    }
}

impl<'db> Extend<&'db ast::Expr> for DunderAll<'db> {
    fn extend<T: IntoIterator<Item = &'db ast::Expr>>(&mut self, iter: T) {
        match self {
            Self::Dynamic => {}
            Self::Static(set) => {
                for item in iter {
                    match item {
                        ast::Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                            set.insert(value.to_str());
                        }
                        _ => {
                            *self = Self::Dynamic;
                            return;
                        }
                    }
                }
            }
            Self::NotPresent => {
                *self = Self::Static(FxHashSet::default());
                self.extend(iter);
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum DefinitionKind<'db> {
    AssignOrAnnAssign,
    Import { alias_name: Option<&'db str> },
    Other,
}
