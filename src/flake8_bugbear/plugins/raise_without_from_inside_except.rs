use rustpython_ast::{ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::python::string::is_lower;

struct RaiseVisitor {
    checks: Vec<Check>,
}

impl<'a> Visitor<'a> for RaiseVisitor {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::Raise {
                exc: Some(exc),
                cause: None,
            } => match &exc.node {
                ExprKind::Name { id, .. } if is_lower(id) => {}
                _ => {
                    self.checks.push(Check::new(
                        CheckKind::RaiseWithoutFromInsideExcept,
                        Range::from_located(stmt),
                    ));
                }
            },
            StmtKind::ClassDef { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::Try { .. } => {}
            StmtKind::If { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::AsyncFor { body, .. } => {
                for stmt in body {
                    self.visit_stmt(stmt);
                }
            }
            _ => {}
        }
    }
}

pub fn raise_without_from_inside_except(checker: &mut Checker, body: &[Stmt]) {
    let mut visitor = RaiseVisitor { checks: vec![] };
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
    checker.add_checks(visitor.checks.into_iter());
}
