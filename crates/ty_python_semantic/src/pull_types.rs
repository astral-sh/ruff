//! A utility visitor for testing, which attempts to "pull a type" for ever sub-node in a given AST.
//!
//! This is used in the "corpus" and (indirectly) the "mdtest" integration tests for this crate.
//! (Mdtest uses the `pull_types` function via the `ty_test` crate.)

use crate::{Db, HasType, SemanticModel};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{
    self as ast, Alias, Comprehension, Expr, Parameter, ParameterWithDefault, Stmt, StmtKind,
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

    fn visit_target(&mut self, target: Expr<'_>) {
        match target.fields() {
            ast::ExprFields::List(ast::ExprListFields { elts, .. })
            | ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }) => {
                for element in elts {
                    self.visit_target(element);
                }
            }
            _ => self.visit_expr(target),
        }
    }
}

impl SourceOrderVisitor<'_> for PullTypesVisitor<'_> {
    fn visit_stmt(&mut self, stmt: Stmt<'_>) {
        match stmt.classify() {
            StmtKind::FunctionDef(function) => {
                let _ty = function.inferred_type(&self.model);
            }
            StmtKind::ClassDef(class) => {
                let _ty = class.inferred_type(&self.model);
            }
            StmtKind::Assign(assign) => {
                for target in assign.targets() {
                    self.visit_target(target);
                }
                self.visit_expr(assign.value());
                return;
            }
            StmtKind::For(for_stmt) => {
                self.visit_target(for_stmt.target());
                self.visit_expr(for_stmt.iter());
                self.visit_body(for_stmt.body());
                self.visit_body(for_stmt.orelse());
                return;
            }
            StmtKind::With(with_stmt) => {
                for item in with_stmt.items() {
                    if let Some(target) = item.optional_vars() {
                        self.visit_target(target);
                    }
                    self.visit_expr(item.context_expr());
                }

                self.visit_body(with_stmt.body());
                return;
            }
            StmtKind::AnnAssign(_)
            | StmtKind::Return(_)
            | StmtKind::Delete(_)
            | StmtKind::AugAssign(_)
            | StmtKind::TypeAlias(_)
            | StmtKind::While(_)
            | StmtKind::If(_)
            | StmtKind::Match(_)
            | StmtKind::Raise(_)
            | StmtKind::Try(_)
            | StmtKind::Assert(_)
            | StmtKind::Import(_)
            | StmtKind::ImportFrom(_)
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_)
            | StmtKind::Expr(_)
            | StmtKind::Pass(_)
            | StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::IpyEscapeCommand(_) => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: Expr<'_>) {
        let _ty = expr.inferred_type(&self.model);

        source_order::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: Comprehension<'_>) {
        self.visit_expr(comprehension.iter());
        self.visit_target(comprehension.target());
        for if_expr in comprehension.ifs() {
            self.visit_expr(if_expr);
        }
    }

    fn visit_parameter(&mut self, parameter: Parameter<'_>) {
        let _ty = parameter.inferred_type(&self.model);

        source_order::walk_parameter(self, parameter);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: ParameterWithDefault<'_>) {
        let _ty = parameter_with_default.inferred_type(&self.model);

        source_order::walk_parameter_with_default(self, parameter_with_default);
    }

    fn visit_alias(&mut self, alias: Alias<'_>) {
        let _ty = alias.inferred_type(&self.model);

        source_order::walk_alias(self, alias);
    }
}
