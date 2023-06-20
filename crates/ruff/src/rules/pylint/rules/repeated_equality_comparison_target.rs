use rustpython_parser::ast::{BoolOp, CmpOp, Expr, ExprCompare};

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

fn is_allowed_op(op: BoolOp) -> bool {
    match op {
        BoolOp::Or => true,
        BoolOp::And => true,
    }
}

/// Return true if the given comparison operator is allowed with the given
/// boolean operator.
///
/// For example,
/// ```python
/// foo == "bar" or foo == "baz" or foo == "qux"
/// ```
/// can be rewritten as
/// ```python
/// foo in {"bar", "baz", "qux"}
/// ```
/// and
/// ```python
/// foo != "bar" or foo != "baz" or foo != "qux"
/// ```
/// can be rewritten as
/// ```python
/// foo not in {"bar", "baz", "qux"}
/// ```
fn is_allowed_compare_op(bool_op: BoolOp, cmp_op: CmpOp) -> bool {
    match bool_op {
        BoolOp::Or => matches!(cmp_op, CmpOp::Eq),
        BoolOp::And => matches!(cmp_op, CmpOp::NotEq),
    }
}

fn is_call(expr: &Expr) -> bool {
    matches!(expr, Expr::Call(_))
}

fn is_allowed_value(bool_op: BoolOp, value: &Expr) -> bool {
    match value {
        Expr::Compare(ExprCompare {
            left,
            ops,
            comparators,
            ..
        }) => {
            for op in ops {
                if !is_allowed_compare_op(bool_op, *op) {
                    return false;
                }
                if is_call(left) {
                    return false;
                }
                for comparator in comparators {
                    if is_call(comparator) {
                        return false;
                    }
                }
            }
            true
        }
        _ => false,
    }
}

/// PLR0124
pub(crate) fn repeated_equality_comparison_target(
    _checker: &mut Checker,
    op: BoolOp,
    values: &[Expr],
) {
    if !is_allowed_op(op) {
        return;
    }
    for value in values {
        if !is_allowed_value(op, value) {
            return;
        }
    }
    // TODO: Check if the expression can be rewritten as a membership test.
}
