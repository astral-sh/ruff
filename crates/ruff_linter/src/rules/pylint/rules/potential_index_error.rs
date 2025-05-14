use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for hard-coded sequence accesses that are known to be out of bounds.
///
/// ## Why is this bad?
/// Attempting to access a sequence with an out-of-bounds index will cause an
/// `IndexError` to be raised at runtime. When the sequence and index are
/// defined statically (e.g., subscripts on `list` and `tuple` literals, with
/// integer indexes), such errors can be detected ahead of time.
///
/// ## Example
/// ```python
/// print([0, 1, 2][3])
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct PotentialIndexError;

impl Violation for PotentialIndexError {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Expression is likely to raise `IndexError`".to_string()
    }
}

/// PLE0643
pub(crate) fn potential_index_error(checker: &Checker, value: &Expr, slice: &Expr) {
    // Determine the length of the sequence.
    let length = match value {
        Expr::Tuple(ast::ExprTuple { elts, .. }) | Expr::List(ast::ExprList { elts, .. }) => {
            match i64::try_from(elts.len()) {
                Ok(length) => length,
                Err(_) => return,
            }
        }
        _ => {
            return;
        }
    };

    // Determine the index value.
    let index = match slice {
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(number_value),
            ..
        }) => number_value.as_i64(),
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            operand,
            ..
        }) => match operand.as_ref() {
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(number_value),
                ..
            }) => number_value.as_i64().map(|number| -number),
            _ => return,
        },
        _ => return,
    };

    // Emit a diagnostic if the index is out of bounds. If the index can't be represented as an
    // `i64`, but the length _can_, then the index is definitely out of bounds.
    if index.is_none_or(|index| index >= length || index < -length) {
        checker.report_diagnostic(Diagnostic::new(PotentialIndexError, slice.range()));
    }
}
