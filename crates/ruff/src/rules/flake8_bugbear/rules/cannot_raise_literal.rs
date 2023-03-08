use rustpython_parser::ast::{Expr, ExprKind};

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

#[violation]
pub struct CannotRaiseLiteral;

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
    checker
        .diagnostics
        .push(Diagnostic::new(CannotRaiseLiteral, Range::from(expr)));
}
