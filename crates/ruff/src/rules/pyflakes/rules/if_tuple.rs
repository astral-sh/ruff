use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct IfTuple;
);
impl Violation for IfTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("If test is a tuple, which is always `True`")
    }
}

/// F634
pub fn if_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            checker
                .diagnostics
                .push(Diagnostic::new(IfTuple, Range::from_located(stmt)));
        }
    }
}
