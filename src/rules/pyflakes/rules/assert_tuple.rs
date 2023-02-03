use crate::define_simple_violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_simple_violation!(
    AssertTuple,
    "Assert test is a non-empty tuple, which is always `True`"
);

/// F631
pub fn assert_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            checker
                .diagnostics
                .push(Diagnostic::new(AssertTuple, Range::from_located(stmt)));
        }
    }
}
