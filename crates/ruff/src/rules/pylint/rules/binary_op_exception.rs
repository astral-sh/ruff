use rustpython_parser::ast::{self, Excepthandler, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Boolop {
    And,
    Or,
}

impl From<&ast::Boolop> for Boolop {
    fn from(op: &ast::Boolop) -> Self {
        match op {
            ast::Boolop::And => Boolop::And,
            ast::Boolop::Or => Boolop::Or,
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
    op: Boolop,
}

impl Violation for BinaryOpException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BinaryOpException { op } = self;
        match op {
            Boolop::And => format!("Exception to catch is the result of a binary `and` operation"),
            Boolop::Or => format!("Exception to catch is the result of a binary `or` operation"),
        }
    }
}

/// PLW0711
pub(crate) fn binary_op_exception(checker: &mut Checker, excepthandler: &Excepthandler) {
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = excepthandler;

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
