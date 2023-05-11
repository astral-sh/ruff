use rustpython_parser::ast::{self, Expr, ExprKind, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct AssertTuple;

impl Violation for AssertTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assert test is a non-empty tuple, which is always `True`")
    }
}

/// F631
pub(crate) fn assert_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let ExprKind::Tuple(ast::ExprTuple { elts, .. }) = &test.node {
        if !elts.is_empty() {
            checker
                .diagnostics
                .push(Diagnostic::new(AssertTuple, stmt.range()));
        }
    }
}
