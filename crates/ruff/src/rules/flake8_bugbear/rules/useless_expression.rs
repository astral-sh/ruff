use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UselessExpression;
);
impl Violation for UselessExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found useless expression. Either assign it to a variable or remove it.")
    }
}

// B018
pub fn useless_expression(checker: &mut Checker, body: &[Stmt]) {
    for stmt in body {
        if let StmtKind::Expr { value } = &stmt.node {
            match &value.node {
                ExprKind::List { .. } | ExprKind::Dict { .. } | ExprKind::Set { .. } => {
                    checker.diagnostics.push(Diagnostic::new(
                        UselessExpression,
                        Range::from_located(value),
                    ));
                }
                ExprKind::Constant { value: val, .. } => match &val {
                    Constant::Str { .. } | Constant::Ellipsis => {}
                    _ => {
                        checker.diagnostics.push(Diagnostic::new(
                            UselessExpression,
                            Range::from_located(value),
                        ));
                    }
                },
                _ => {}
            }
        }
    }
}
