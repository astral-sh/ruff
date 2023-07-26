use std::hash::BuildHasherDefault;
use std::ops::Deref;

use itertools::{any, Itertools};
use ruff_python_ast::{BoolOp, CmpOp, Expr, ExprBoolOp, ExprCompare, Ranged};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::hashable::HashableExpr;
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for repeated equality comparisons that can rewritten as a membership
/// test.
///
/// ## Why is this bad?
/// To check if a variable is equal to one of many values, it is common to
/// write a series of equality comparisons (e.g.,
/// `foo == "bar" or foo == "baz"`).
///
/// Instead, prefer to combine the values into a collection and use the `in`
/// operator to check for membership, which is more performant and succinct.
/// If the items are hashable, use a `set` for efficiency; otherwise, use a
/// `tuple`.
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
pub struct RepeatedEqualityComparisonTarget {
    expr: String,
}

impl Violation for RepeatedEqualityComparisonTarget {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RepeatedEqualityComparisonTarget { expr } = self;
        format!(
            "Consider merging multiple comparisons: `{expr}`. Use a `set` if the elements are hashable."
        )
    }
}

/// PLR1714
pub(crate) fn repeated_equality_comparison_target(checker: &mut Checker, bool_op: &ExprBoolOp) {
    if bool_op
        .values
        .iter()
        .any(|value| !is_allowed_value(bool_op.op, value))
    {
        return;
    }

    let mut left_to_comparators: FxHashMap<HashableExpr, (usize, Vec<&Expr>)> =
        FxHashMap::with_capacity_and_hasher(bool_op.values.len(), BuildHasherDefault::default());
    for value in &bool_op.values {
        if let Expr::Compare(ExprCompare {
            left, comparators, ..
        }) = value
        {
            let (count, matches) = left_to_comparators
                .entry(left.deref().into())
                .or_insert_with(|| (0, Vec::new()));
            *count += 1;
            matches.extend(comparators);
        }
    }

    for (left, (count, comparators)) in left_to_comparators {
        if count > 1 {
            checker.diagnostics.push(Diagnostic::new(
                RepeatedEqualityComparisonTarget {
                    expr: merged_membership_test(
                        left.as_expr(),
                        bool_op.op,
                        &comparators,
                        checker.locator(),
                    ),
                },
                bool_op.range(),
            ));
        }
    }
}

/// Return `true` if the given expression is compatible with a membership test.
/// E.g., `==` operators can be joined with `or` and `!=` operators can be
/// joined with `and`.
fn is_allowed_value(bool_op: BoolOp, value: &Expr) -> bool {
    let Expr::Compare(ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = value
    else {
        return false;
    };

    ops.iter().all(|op| {
        if match bool_op {
            BoolOp::Or => !matches!(op, CmpOp::Eq),
            BoolOp::And => !matches!(op, CmpOp::NotEq),
        } {
            return false;
        }

        if left.is_call_expr() {
            return false;
        }

        if any(comparators.iter(), Expr::is_call_expr) {
            return false;
        }

        true
    })
}

/// Generate a string like `obj in (a, b, c)` or `obj not in (a, b, c)`.
fn merged_membership_test(
    left: &Expr,
    op: BoolOp,
    comparators: &[&Expr],
    locator: &Locator,
) -> String {
    let op = match op {
        BoolOp::Or => "in",
        BoolOp::And => "not in",
    };
    let left = locator.slice(left.range());
    let members = comparators
        .iter()
        .map(|comparator| locator.slice(comparator.range()))
        .join(", ");
    format!("{left} {op} ({members})",)
}
