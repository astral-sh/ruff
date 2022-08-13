use rustpython_parser::ast::{ExprKind, Stmt, StmtKind, Suite};

use crate::check::{Check, CheckKind};
use crate::visitor::{walk_stmt, Visitor};

struct Checker {
    checks: Vec<Check>,
}

impl Visitor for Checker {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::ImportFrom { names, .. } => {
                for alias in names {
                    if alias.name == "*" {
                        self.checks.push(Check {
                            kind: CheckKind::ImportStarUsage,
                            location: stmt.location,
                        });
                    }
                }
            }
            StmtKind::If { test, .. } => {
                if let ExprKind::Tuple { .. } = test.node {
                    self.checks.push(Check {
                        kind: CheckKind::IfTuple,
                        location: stmt.location,
                    });
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

pub fn check_ast(python_ast: &Suite) -> Vec<Check> {
    python_ast
        .iter()
        .flat_map(|stmt| {
            let mut checker = Checker { checks: vec![] };
            checker.visit_stmt(stmt);
            checker.checks
        })
        .collect()
}
