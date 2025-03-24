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
use rustc_hash::FxHashSet;

use crate::{module_name::ModuleName, resolve_module, Db};

fn exports_cycle_recover(
    _db: &dyn Db,
    _value: &FxHashSet<Name>,
    _count: u32,
    _file: File,
) -> salsa::CycleRecoveryAction<FxHashSet<Name>> {
    salsa::CycleRecoveryAction::Iterate
}

fn exports_cycle_initial(_db: &dyn Db, _file: File) -> FxHashSet<Name> {
    FxHashSet::default()
}

#[salsa::tracked(return_ref, cycle_fn=exports_cycle_recover, cycle_initial=exports_cycle_initial)]
pub(super) fn exported_names(db: &dyn Db, file: File) -> FxHashSet<Name> {
    let module = parsed_module(db.upcast(), file);
    let mut finder = ExportFinder::new(db, file);
    finder.visit_body(module.suite());
    finder.exports
}

struct ExportFinder<'db> {
    db: &'db dyn Db,
    file: File,
    visiting_stub_file: bool,
    exports: FxHashSet<Name>,
}

impl<'db> ExportFinder<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            db,
            file,
            visiting_stub_file: file.is_stub(db.upcast()),
            exports: FxHashSet::default(),
        }
    }

    fn possibly_add_export(&mut self, name: &Name) {
        if name.starts_with('_') {
            return;
        }
        self.exports.insert(name.clone());
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias { name, asname, .. } = alias;
        if self.visiting_stub_file {
            // If the source is a stub, names defined by imports are only exported
            // if they use the explicit `foo as foo` syntax:
            if asname.as_ref().is_some_and(|asname| asname.id == name.id) {
                self.possibly_add_export(&name.id);
            }
        } else {
            self.possibly_add_export(&asname.as_ref().unwrap_or(name).id);
        }
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
                        self.possibly_add_export(&name.id);
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
                    self.possibly_add_export(&rest.id);
                }
            }
            ast::Pattern::MatchStar(ast::PatternMatchStar { name, range: _ }) => {
                if let Some(name) = name {
                    self.possibly_add_export(&name.id);
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

    fn visit_stmt(&mut self, stmt: &'db ruff_python_ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                decorator_list,
                arguments,
                type_params: _, // We don't want to visit the type params of the class
                body: _,        // We don't want to visit the body of the class
                range: _,
            }) => {
                self.possibly_add_export(&name.id);
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
                self.possibly_add_export(&name.id);
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
                            self.exports.extend(
                                ModuleName::from_import_statement(self.db, self.file, node)
                                    .ok()
                                    .and_then(|module_name| resolve_module(self.db, &module_name))
                                    .iter()
                                    .flat_map(|module| exported_names(self.db, module.file()))
                                    .cloned(),
                            );
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
            ast::Expr::Name(ast::ExprName { id, ctx, range: _ }) => {
                if ctx.is_store() {
                    self.possibly_add_export(id);
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
                    self.export_finder.possibly_add_export(id);
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
