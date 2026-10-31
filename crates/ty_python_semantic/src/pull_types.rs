//! A utility visitor for testing, which attempts to "pull a type" for ever sub-node in a given AST.
//!
//! This is used in the "corpus" and (indirectly) the "mdtest" integration tests for this crate.
//! (Mdtest uses the `pull_types` function via the `ty_test` crate.)

use crate::{Db, HasType, SemanticModel};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::{
    Alias, Comprehension, Expr, Parameter, ParameterWithDefault, Stmt, flat::visitor::source_order,
    flat::visitor::source_order::SourceOrderVisitor,
};
use ty_python_core::ProgramFile;

pub fn pull_types(db: &dyn Db, file: ProgramFile<'_>) {
    let mut visitor = PullTypesVisitor::new(db, file);

    let ast = parsed_module(db, file.python_file(db)).load(db);

    visitor.visit_body(ast.suite());
}

struct PullTypesVisitor<'db> {
    model: SemanticModel<'db>,
}

impl<'db> PullTypesVisitor<'db> {
    fn new(db: &'db dyn Db, file: ProgramFile<'db>) -> Self {
        Self {
            model: SemanticModel::new(db, file),
        }
    }

    fn visit_target(&mut self, target: flat::View<'_, Expr>) {
        match target.fields() {
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
    fn visit_stmt(&mut self, stmt: flat::View<'_, Stmt>) {
        match stmt.classify() {
            flat::StmtKind::FunctionDef(function) => {
                let _ty = function.inferred_type(&self.model);
            }
            flat::StmtKind::ClassDef(class) => {
                let _ty = class.inferred_type(&self.model);
            }
            flat::StmtKind::Assign(assign) => {
                for target in assign.targets() {
                    self.visit_target(target);
                }
                self.visit_expr(assign.value());
                return;
            }
            flat::StmtKind::For(for_stmt) => {
                self.visit_target(for_stmt.target());
                self.visit_expr(for_stmt.iter());
                self.visit_body(for_stmt.body());
                self.visit_body(for_stmt.orelse());
                return;
            }
            flat::StmtKind::With(with_stmt) => {
                for item in with_stmt.items() {
                    if let Some(target) = item.optional_vars() {
                        self.visit_target(target);
                    }
                    self.visit_expr(item.context_expr());
                }

                self.visit_body(with_stmt.body());
                return;
            }
            flat::StmtKind::AnnAssign(_)
            | flat::StmtKind::Return(_)
            | flat::StmtKind::Delete(_)
            | flat::StmtKind::AugAssign(_)
            | flat::StmtKind::TypeAlias(_)
            | flat::StmtKind::While(_)
            | flat::StmtKind::If(_)
            | flat::StmtKind::Match(_)
            | flat::StmtKind::Raise(_)
            | flat::StmtKind::Try(_)
            | flat::StmtKind::Assert(_)
            | flat::StmtKind::Import(_)
            | flat::StmtKind::ImportFrom(_)
            | flat::StmtKind::Global(_)
            | flat::StmtKind::Nonlocal(_)
            | flat::StmtKind::Expr(_)
            | flat::StmtKind::Pass(_)
            | flat::StmtKind::Break(_)
            | flat::StmtKind::Continue(_)
            | flat::StmtKind::IpyEscapeCommand(_) => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: flat::View<'_, Expr>) {
        let _ty = expr.inferred_type(&self.model);

        source_order::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: flat::View<'_, Comprehension>) {
        self.visit_expr(comprehension.iter());
        self.visit_target(comprehension.target());
        for if_expr in comprehension.ifs() {
            self.visit_expr(if_expr);
        }
    }

    fn visit_parameter(&mut self, parameter: flat::View<'_, Parameter>) {
        let _ty = parameter.inferred_type(&self.model);

        source_order::walk_parameter(self, parameter);
    }

    fn visit_parameter_with_default(
        &mut self,
        parameter_with_default: flat::View<'_, ParameterWithDefault>,
    ) {
        let _ty = parameter_with_default.inferred_type(&self.model);

        source_order::walk_parameter_with_default(self, parameter_with_default);
    }

    fn visit_alias(&mut self, alias: flat::View<'_, Alias>) {
        let _ty = alias.inferred_type(&self.model);

        source_order::walk_alias(self, alias);
    }
}
