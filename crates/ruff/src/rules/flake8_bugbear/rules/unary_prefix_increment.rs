use rustpython_parser::ast::{self, Expr, Ranged, UnaryOp};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the unary prefix increment operator (e.g., `++n`).
///
/// ## Why is this bad?
/// Python does not support the unary prefix increment operator. Writing `++n`
/// is equivalent to `+(+(n))`, which is equivalent to `n`.
///
/// ## Example
/// ```python
/// ++n
/// ```
///
/// Use instead:
/// ```python
/// n += 1
/// ```
///
/// ## References
/// - [Python documentation: Unary arithmetic and bitwise operations](https://docs.python.org/3/reference/expressions.html#unary-arithmetic-and-bitwise-operations)
/// - [Python documentation: Augmented assignment statements](https://docs.python.org/3/reference/simple_stmts.html#augmented-assignment-statements)
#[violation]
pub struct UnaryPrefixIncrement;

impl Violation for UnaryPrefixIncrement {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Python does not support the unary prefix increment")
    }
}

/// B002
pub(crate) fn unary_prefix_increment(
    checker: &mut Checker,
    expr: &Expr,
    op: UnaryOp,
    operand: &Expr,
) {
    if !matches!(op, UnaryOp::UAdd) {
        return;
    }
    let Expr::UnaryOp(ast::ExprUnaryOp { op, .. }) = operand else {
        return;
    };
    if !matches!(op, UnaryOp::UAdd) {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(UnaryPrefixIncrement, expr.range()));
}
