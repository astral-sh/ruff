use ruff_python_ast::{self as ast, ExceptHandler, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum BoolOp {
    And,
    Or,
}

impl From<&ast::BoolOp> for BoolOp {
    fn from(op: &ast::BoolOp) -> Self {
        match op {
            ast::BoolOp::And => BoolOp::And,
            ast::BoolOp::Or => BoolOp::Or,
        }
    }
}

/// ## What it does
/// Checks for `except` clauses that attempt to catch multiple
/// exceptions with a binary operation (`and` or `or`).
///
/// ## Why is this bad?
/// A binary operation will not catch multiple exceptions. Instead, the binary
/// operation will be evaluated first, and the result of _that_ operation will
/// be caught (for an `or` operation, this is typically the first exception in
/// the list). This is almost never the desired behavior.
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
/// except (A, B):
///     pass
/// ```
#[violation]
pub struct BinaryOpException {
    op: BoolOp,
}

impl Violation for BinaryOpException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BinaryOpException { op } = self;
        match op {
            BoolOp::And => format!("Exception to catch is the result of a binary `and` operation"),
            BoolOp::Or => format!("Exception to catch is the result of a binary `or` operation"),
        }
    }
}

/// PLW0711
pub(crate) fn binary_op_exception(checker: &mut Checker, except_handler: &ExceptHandler) {
    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) =
        except_handler;

    let Some(type_) = type_ else {
        return;
    };

    let Expr::BoolOp(ast::ExprBoolOp { op, .. }) = type_.as_ref() else {
        return;
    };

    checker.diagnostics.push(Diagnostic::new(
        BinaryOpException { op: op.into() },
        type_.range(),
    ));
}
