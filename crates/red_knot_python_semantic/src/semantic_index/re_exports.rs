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

use crate::{module_name::ModuleName, resolve_module, Db};

#[salsa::tracked(return_ref)]
pub(super) fn exported_names(db: &dyn Db, file: File) -> FxHashSet<Name> {
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

impl ExportFinder<'_> {
    fn possibly_add_export(&mut self, name: &Name) {
        if name.starts_with('_') {
            return;
        }
        if !self.exports.contains(name) {
            self.exports.insert(name.clone());
        }
    }
}

impl<'db> Visitor<'db> for ExportFinder<'db> {
    fn visit_alias(&mut self, alias: &'db ast::Alias) {
        let ast::Alias { name, asname, .. } = alias;
        if self.file.is_stub(self.db.upcast()) {
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
                if value.is_some() || self.file.is_stub(self.db.upcast()) {
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

            ast::Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
            })
            | ast::Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
            })
            | ast::Expr::Generator(ast::ExprGenerator {
                elt,
                generators,
                range: _,
                parenthesized: _,
            }) => {
                let mut walrus_finder = WalrusFinder {
                    export_finder: self,
                };
                walrus_finder.visit_expr(elt);
                for generator in generators {
                    walrus_finder.visit_comprehension(generator);
                }
            }

            ast::Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => {
                let mut walrus_finder = WalrusFinder {
                    export_finder: self,
                };
                walrus_finder.visit_expr(key);
                walrus_finder.visit_expr(value);
                for generator in generators {
                    walrus_finder.visit_comprehension(generator);
                }
            }

            _ => walk_expr(self, expr),
        }
    }
}

struct WalrusFinder<'a, 'db> {
    export_finder: &'a mut ExportFinder<'db>,
}

impl<'db> Visitor<'db> for WalrusFinder<'_, 'db> {
    fn visit_expr(&mut self, expr: &'db ast::Expr) {
        match expr {
            ast::Expr::DictComp(_)
            | ast::Expr::SetComp(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::Generator(_) => {}

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

            _ => walk_expr(self, expr),
        }
    }
}
