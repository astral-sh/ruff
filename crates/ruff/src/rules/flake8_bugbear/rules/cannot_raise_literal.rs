use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct CannotRaiseLiteral;

impl Violation for CannotRaiseLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot raise a literal. Did you intend to return it or raise an Exception?")
    }
}

/// B016
pub(crate) fn cannot_raise_literal(checker: &mut Checker, expr: &Expr) {
    let Expr::Constant ( _) = expr else {
        return;
    };
    checker
        .diagnostics
        .push(Diagnostic::new(CannotRaiseLiteral, expr.range()));
}
