use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "0.5.0", category = Category::Correctness)]
pub(crate) struct PotentialIndexError;

impl Violation for PotentialIndexError {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Expression is likely to raise `IndexError`".to_string()
    }
}

/// PLE0643
pub(crate) fn potential_index_error(checker: &Checker, value: Expr<'_>, slice: Expr<'_>) {
    // Determine the length of the sequence.
    let Some(elts) = value.as_list_or_tuple_elements() else {
        return;
    };
    if elts.iter().any(Expr::is_starred_expr) {
        return;
    }
    let Ok(length) = i64::try_from(elts.len()) else {
        return;
    };

    // Determine the index value.
    let index = match slice.classify() {
        ast::ExprKind::NumberLiteral(number) if let Some(int) = number.value().as_int() => {
            int.as_i64()
        }
        ast::ExprKind::UnaryOp(unary)
            if unary.op() == ast::UnaryOp::USub
                && let Some(int) = unary.operand().as_int_literal() =>
        {
            int.as_i64().map(|number| -number)
        }
        _ => return,
    };

    // Emit a diagnostic if the index is out of bounds. If the index can't be represented as an
    // `i64`, but the length _can_, then the index is definitely out of bounds.
    if index.is_none_or(|index| index >= length || index < -length) {
        checker.report_diagnostic(PotentialIndexError, slice.range());
    }
}
