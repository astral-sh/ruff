use std::hash::BuildHasherDefault;
use std::ops::Deref;

use itertools::{any, Itertools};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::hashable::HashableExpr;
use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextSize};

use crate::autofix::snippet::SourceCodeSnippet;
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
pub struct RepeatedEqualityComparison {
    expression: SourceCodeSnippet,
}

impl Violation for RepeatedEqualityComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RepeatedEqualityComparison { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!(
                "Consider merging multiple comparisons: `{expression}`. Use a `set` if the elements are hashable."
            )
        } else {
            format!(
                "Consider merging multiple comparisons. Use a `set` if the elements are hashable."
            )
        }
    }
}

/// PLR1714
pub(crate) fn repeated_equality_comparison(checker: &mut Checker, bool_op: &ast::ExprBoolOp) {
    if bool_op
        .values
        .iter()
        .any(|value| !is_allowed_value(bool_op.op, value))
    {
        return;
    }

    // Map from expression hash to (starting offset, number of comparisons, list
    let mut value_to_comparators: FxHashMap<HashableExpr, (TextSize, Vec<&Expr>)> =
        FxHashMap::with_capacity_and_hasher(
            bool_op.values.len() * 2,
            BuildHasherDefault::default(),
        );

    for value in &bool_op.values {
        // Enforced via `is_allowed_value`.
        let Expr::Compare(ast::ExprCompare {
            left, comparators, ..
        }) = value
        else {
            return;
        };

        // Enforced via `is_allowed_value`.
        let [right] = comparators.as_slice() else {
            return;
        };

        if matches!(left.as_ref(), Expr::Name(_) | Expr::Attribute(_)) {
            let (_, left_matches) = value_to_comparators
                .entry(left.deref().into())
                .or_insert_with(|| (left.start(), Vec::new()));
            left_matches.push(right);
        }

        if matches!(right, Expr::Name(_) | Expr::Attribute(_)) {
            let (_, right_matches) = value_to_comparators
                .entry(right.into())
                .or_insert_with(|| (right.start(), Vec::new()));
            right_matches.push(left);
        }
    }

    for (value, (_, comparators)) in value_to_comparators
        .iter()
        .sorted_by_key(|(_, (start, _))| *start)
    {
        if comparators.len() > 1 {
            checker.diagnostics.push(Diagnostic::new(
                RepeatedEqualityComparison {
                    expression: SourceCodeSnippet::new(merged_membership_test(
                        value.as_expr(),
                        bool_op.op,
                        comparators,
                        checker.locator(),
                    )),
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
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = value
    else {
        return false;
    };

    // Ignore, e.g., `foo == bar == baz`.
    let [op] = ops.as_slice() else {
        return false;
    };

    if match bool_op {
        BoolOp::Or => !matches!(op, CmpOp::Eq),
        BoolOp::And => !matches!(op, CmpOp::NotEq),
    } {
        return false;
    }

    // Ignore self-comparisons, e.g., `foo == foo`.
    let [right] = comparators.as_slice() else {
        return false;
    };
    if ComparableExpr::from(left) == ComparableExpr::from(right) {
        return false;
    }

    if left.is_call_expr() {
        return false;
    }

    if any(comparators.iter(), Expr::is_call_expr) {
        return false;
    }

    true
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
    let left = locator.slice(left);
    let members = comparators
        .iter()
        .map(|comparator| locator.slice(comparator))
        .join(", ");
    format!("{left} {op} ({members})",)
}
