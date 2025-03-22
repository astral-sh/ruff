//! A visitor and query to find all global-scope symbols that are exported from a module
//! when a wildcard import is used.
//!
//! For example, if a module `foo` contains `from bar import *`, which symbols from the global
//! scope of `bar` are imported into the global namespace of `foo`?

use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast,
    name::Name,
    visitor::{walk_expr, walk_pattern, walk_stmt, Visitor},
};
use rustc_hash::FxHashSet;

use crate::{module_name::module_name_from_import_statement, resolve_module, Db};

#[salsa::tracked(return_ref)]
pub(super) fn find_exports(db: &dyn Db, file: File) -> FxHashSet<Name> {
    let module = parsed_module(db.upcast(), file);

    let mut finder = ExportFinder {
        db,
        file,
        exports: FxHashSet::default(),
    };

    finder.visit_body(module.suite());
    finder.exports
}

struct ExportFinder<'db> {
    db: &'db dyn Db,
    file: File,
    exports: FxHashSet<Name>,
}

impl<'db> ExportFinder<'db> {
    fn possibly_add_export(&mut self, name: &Name) {
        if name.starts_with('_') {
            return;
        }
        if !self.exports.contains(name) {
            self.exports.insert(name.clone());
        }
    }

    fn visit_target(&mut self, target: &'db ast::Expr) {
        match target {
            ast::Expr::Name(ast::ExprName { id, .. }) => {
                self.possibly_add_export(id);
            }
            ast::Expr::Tuple(tuple) => {
                for element in tuple {
                    self.visit_target(element);
                }
            }
            ast::Expr::List(list) => {
                for element in list {
                    self.visit_target(element);
                }
            }
            ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.visit_target(value);
            }
            _ => self.visit_expr(target),
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias { name, asname, .. } = alias;
        if self.file.is_stub(self.db.upcast()) {
            // If the source is a stub, names defined by imports are only exported
            // if they use the explicit `foo as foo` syntax:
            if asname.as_ref() == Some(name) {
                self.possibly_add_export(&name.id);
            }
        } else {
            self.possibly_add_export(&name.id);
        }
    }

    fn visit_with_item(&mut self, with_item: &'db ast::WithItem) {
        let ast::WithItem {
            context_expr,
            optional_vars,
            range: _,
        } = with_item;

        if let Some(var) = optional_vars.as_deref() {
            self.visit_target(var);
        }
        self.visit_expr(context_expr);
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
                type_params,
                arguments,
                body: _, // We don't want to visit the body of the class
                range: _,
            }) => {
                self.possibly_add_export(&name.id);
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = type_params {
                    self.visit_type_params(type_params);
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
                type_params,
                body: _, // We don't want to visit the body of the function
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
                if let Some(type_params) = type_params {
                    self.visit_type_params(type_params);
                }
            }
            ast::Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                range: _,
            }) => {
                for target in targets {
                    self.visit_target(target);
                }
                self.visit_expr(value);
            }
            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value,
                annotation,
                simple: _,
                range: _,
            }) => {
                if value.is_some() || self.file.is_stub(self.db.upcast()) {
                    self.visit_target(target);
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
                self.visit_target(name);
                // Neither walrus expressions nor statements cannot appear in type aliases;
                // no need to recursively visit the `value` or `type_params`
            }
            ast::Stmt::For(ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                range: _,
                is_async: _,
            }) => {
                self.visit_target(target);
                self.visit_expr(iter);
                for child in body {
                    self.visit_stmt(child);
                }
                for child in orelse {
                    self.visit_stmt(child);
                }
            }
            ast::Stmt::ImportFrom(node) => {
                for name in &node.names {
                    if &name.name.id == "*" {
                        self.exports.extend(
                            module_name_from_import_statement(self.db, self.file, node)
                                .ok()
                                .and_then(|module_name| resolve_module(self.db, &module_name))
                                .iter()
                                .flat_map(|module| find_exports(self.db, module.file()))
                                .cloned(),
                        );
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
            ast::Expr::Named(ast::ExprNamed {
                target,
                value,
                range: _,
            }) => {
                if let ast::Expr::Name(ast::ExprName { id, .. }) = &**target {
                    self.possibly_add_export(id);
                } else {
                    self.visit_expr(target);
                }
                self.visit_expr(value);
            }
            _ => walk_expr(self, expr),
        }
    }
}
