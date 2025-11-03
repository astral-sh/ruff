//! A utility visitor for testing, which attempts to "pull a type" for ever sub-node in a given AST.
//!
//! This is used in the "corpus" and (indirectly) the "mdtest" integration tests for this crate.
//! (Mdtest uses the `pull_types` function via the `ty_test` crate.)

use crate::{Db, HasType, SemanticModel};
use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    self as ast, visitor::source_order, visitor::source_order::SourceOrderVisitor,
};

pub fn pull_types(db: &dyn Db, file: File) {
    let mut visitor = PullTypesVisitor::new(db, file);

    let ast = parsed_module(db, file).load(db);

    visitor.visit_body(ast.suite());
}

struct PullTypesVisitor<'db> {
    model: SemanticModel<'db>,
}

impl<'db> PullTypesVisitor<'db> {
    fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            model: SemanticModel::new(db, file),
        }
    }

    fn visit_target(&mut self, target: &ast::Expr) {
        match target {
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for element in elts {
                    self.visit_target(element);
                }
            }
            _ => self.visit_expr(target),
        }
    }
}

impl SourceOrderVisitor<'_> for PullTypesVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(function) => {
                let _ty = function.inferred_type(&self.model);
            }
            ast::Stmt::ClassDef(class) => {
                let _ty = class.inferred_type(&self.model);
            }
            ast::Stmt::Assign(assign) => {
                for target in &assign.targets {
                    self.visit_target(target);
                }
                self.visit_expr(&assign.value);
                return;
            }
            ast::Stmt::For(for_stmt) => {
                self.visit_target(&for_stmt.target);
                self.visit_expr(&for_stmt.iter);
                self.visit_body(&for_stmt.body);
                self.visit_body(&for_stmt.orelse);
                return;
            }
            ast::Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    if let Some(target) = &item.optional_vars {
                        self.visit_target(target);
                    }
                    self.visit_expr(&item.context_expr);
                }

                self.visit_body(&with_stmt.body);
                return;
            }
            ast::Stmt::AnnAssign(_)
            | ast::Stmt::Return(_)
            | ast::Stmt::Delete(_)
            | ast::Stmt::AugAssign(_)
            | ast::Stmt::TypeAlias(_)
            | ast::Stmt::While(_)
            | ast::Stmt::If(_)
            | ast::Stmt::Match(_)
            | ast::Stmt::Raise(_)
            | ast::Stmt::Try(_)
            | ast::Stmt::Assert(_)
            | ast::Stmt::Import(_)
            | ast::Stmt::ImportFrom(_)
            | ast::Stmt::Global(_)
            | ast::Stmt::Nonlocal(_)
            | ast::Stmt::Expr(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::IpyEscapeCommand(_) => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        let _ty = expr.inferred_type(&self.model);

        source_order::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: &ast::Comprehension) {
        self.visit_expr(&comprehension.iter);
        self.visit_target(&comprehension.target);
        for if_expr in &comprehension.ifs {
            self.visit_expr(if_expr);
        }
    }

    fn visit_parameter(&mut self, parameter: &ast::Parameter) {
        let _ty = parameter.inferred_type(&self.model);

        source_order::walk_parameter(self, parameter);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: &ast::ParameterWithDefault) {
        let _ty = parameter_with_default.inferred_type(&self.model);

        source_order::walk_parameter_with_default(self, parameter_with_default);
    }

    fn visit_alias(&mut self, alias: &ast::Alias) {
        let _ty = alias.inferred_type(&self.model);

        source_order::walk_alias(self, alias);
    }
}
