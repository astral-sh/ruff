use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct UselessComparison;

impl Violation for UselessComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Pointless comparison. This comparison does nothing but waste CPU instructions. \
             Either prepend `assert` or remove it."
        )
    }
}

/// B015
pub(crate) fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if matches!(expr.node, ExprKind::Compare(_)) {
        checker
            .diagnostics
            .push(Diagnostic::new(UselessComparison, expr.range()));
    }
}
