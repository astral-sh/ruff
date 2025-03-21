//! A visitor and query to find all global-scope symbols that are exported from a module
//! when a wildcard import is used.
//!
//! For example, if a module `foo` contains `from bar import *`, which symbols from the global
//! scope of `bar` are imported into the global namespace of `foo`?

use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast,
    name::Name,
    visitor::{walk_expr, walk_stmt, Visitor},
};
use rustc_hash::FxHashSet;

use crate::Db;

#[salsa::tracked(return_ref)]
fn find_exports(db: &dyn Db, file: File) -> FxHashSet<Name> {
    let module = parsed_module(db.upcast(), file);

    let mut finder = ExportFinder {
        exports: FxHashSet::default(),
        is_stub: file.is_stub(db.upcast()),
    };

    finder.visit_body(module.suite());
    finder.exports
}

struct ExportFinder {
    exports: FxHashSet<Name>,
    is_stub: bool,
}

impl ExportFinder {
    fn possibly_add_export(&mut self, name: &Name) {
        if name.starts_with('_') {
            return;
        }
        if !self.exports.contains(name) {
            self.exports.insert(name.clone());
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias { name, asname, .. } = alias;
        if self.is_stub {
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
            if let ast::Expr::Name(ast::ExprName { id, .. }) = var {
                self.possibly_add_export(id);
            } else {
                self.visit_expr(var);
            }
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
            ast::Pattern::MatchClass(ast::PatternMatchClass {
                arguments,
                cls: _,
                range: _,
            }) => {
                let ast::PatternArguments {
                    patterns,
                    keywords,
                    range: _,
                } = arguments;
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
                for ast::PatternKeyword {
                    pattern,
                    attr: _,
                    range: _,
                } in keywords
                {
                    self.visit_pattern(pattern);
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
            ast::Pattern::MatchSequence(ast::PatternMatchSequence { patterns, range: _ })
            | ast::Pattern::MatchOr(ast::PatternMatchOr { patterns, range: _ }) => {
                for pattern in patterns {
                    self.visit_pattern(pattern);
                }
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
                    if let ast::Expr::Name(ast::ExprName { id, .. }) = target {
                        self.possibly_add_export(id);
                    } else {
                        self.visit_expr(target);
                    }
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
                if let ast::Expr::Name(ast::ExprName { id, .. }) = &**target {
                    self.possibly_add_export(id);
                } else {
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
                if let ast::Expr::Name(ast::ExprName { id, .. }) = &**name {
                    self.possibly_add_export(id);
                }
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
                if let ast::Expr::Name(ast::ExprName { id, .. }) = &**target {
                    self.possibly_add_export(id);
                }
                self.visit_expr(iter);
                for child in body {
                    self.visit_stmt(child);
                }
                for child in orelse {
                    self.visit_stmt(child);
                }
            }

            ast::Stmt::Import(_)
            | ast::Stmt::AugAssign(_)
            | ast::Stmt::ImportFrom(_)
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
        if let ast::Expr::Named(ast::ExprNamed {
            target,
            value,
            range: _,
        }) = expr
        {
            if let ast::Expr::Name(ast::ExprName { id, .. }) = &**target {
                self.possibly_add_export(id);
            } else {
                self.visit_expr(target);
            }
            self.visit_expr(value);
        } else {
            walk_expr(self, expr);
        }
    }
}
