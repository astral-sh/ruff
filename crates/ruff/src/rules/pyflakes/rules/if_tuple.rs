use rustpython_parser::ast::{self, Expr, ExprKind, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct IfTuple;

impl Violation for IfTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("If test is a tuple, which is always `True`")
    }
}

/// F634
pub(crate) fn if_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let ExprKind::Tuple(ast::ExprTuple { elts, .. }) = &test.node {
        if !elts.is_empty() {
            checker
                .diagnostics
                .push(Diagnostic::new(IfTuple, stmt.range()));
        }
    }
}
