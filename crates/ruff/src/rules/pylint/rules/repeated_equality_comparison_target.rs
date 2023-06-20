use rustpython_parser::ast::{Boolop, Expr, ExprBoolOp};

use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for equality comparisons that can rewritten as a membership test.
///
/// ## Why is this bad?
/// Instead, to check if a variable is equal to one of many values, combine the
/// values into a collection and use the `in` operator. This is faster and less
/// verbose. If the items are hashable, use a `set` instead of a `list`, as
/// membership tests are faster for sets.
///
/// ## Example
/// ```python
/// foo == "bar" or foo == "baz" or foo == "qux"
/// ```
///
/// Use instead:
/// ```python
/// foo in {"bar", "baz", "qux"}
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[violation]
pub struct RepeatedEqualityComparisonTarget;

impl Violation for RepeatedEqualityComparisonTarget {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            // TODO: Improve the message.
            "Consider merging multiple comparisons with ???. \
            Use a `set` if the elements are hashable."
        )
    }
}

fn is_allowed_op(op: Boolop) -> bool {
    op == Boolop::Or || op == Boolop::And
}

/// PLR0124
pub(crate) fn repeated_equality_comparison_target(
    _checker: &mut Checker,
    op: Boolop,
    values: &[Expr],
) {
    // Ignore if not all `or`, these cannot be rewritten as an `in` expression.
    if !is_allowed_op(op) {
        return;
    }
    for value in values {
        if let Expr::BoolOp(ExprBoolOp { op, .. }) = value {
            if !is_allowed_op(*op) {
                return;
            }
        }
    }
    // TODO: Check if the expression can be rewritten as a membership test.
}
