use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct CannotRaiseLiteral;
);
impl Violation for CannotRaiseLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot raise a literal. Did you intend to return it or raise an Exception?")
    }
}

/// B016
pub fn cannot_raise_literal(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Constant { .. } = &expr.node else {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        CannotRaiseLiteral,
        Range::from_located(expr),
    ));
}
