use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` clauses that attempts to catch multiple
/// exceptions with a binary operation (`or`, `and`)
///
/// ## Why is this bad?
/// A binary operation will not catch multiple exceptions. Only the first exception is caught when an `or` is use
/// and using an `and` produces unintended results.
///
/// ## Example
/// ```python
/// try:
///     pass
/// except A or B:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// try:
///     pass
/// except (A,B):
///     pass
/// ```
#[violation]
pub struct BinaryOpException;

impl Violation for BinaryOpException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception to catch is the result of a binary operation")
    }
}

/// PLW0711
pub fn binary_op_exception(checker: &mut Checker, excepthandler: &Excepthandler) {
    let ExcepthandlerKind::ExceptHandler { type_, .. } = &excepthandler.node;

    if let ExprKind::BoolOp { .. } = &type_.as_ref().unwrap().node {
        checker.diagnostics.push(Diagnostic::new(
            BinaryOpException,
            Range::from(excepthandler),
        ));
    };
}
