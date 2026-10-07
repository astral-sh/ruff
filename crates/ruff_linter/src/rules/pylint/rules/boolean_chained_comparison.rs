use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    BoolOp, CmpOp, Expr, ExprBoolOp, ExprCompare,
    token::{parentheses_iterator, parenthesized_range},
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::preview::is_boolean_chained_comparison_reversed_enabled;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Check for chained boolean operations that can be simplified.
///
/// ## Why is this bad?
/// Refactoring the code will improve readability for these cases.
///
/// ## Example
///
/// ```python
/// a = int(input())
/// b = int(input())
/// c = int(input())
/// if a < b and b < c:
///     pass
/// ```
///
/// Use instead:
///
/// ```python
/// a = int(input())
/// b = int(input())
/// c = int(input())
/// if a < b < c:
///     pass
/// ```
///
/// In [preview], this rule also detects comparisons whose order is reversed or
/// whose operators are flipped, such as `b < c and a < b` or `b > a and b < c`.
///
/// ## Fix safety
/// The fix is safe for comparisons that already appear in chain order, such as
/// `a < b and b < c`.
///
/// The fix for reversed or flipped comparisons is marked as unsafe. Rewriting
/// `b < c and a < b` as `a < b < c` changes the order in which the operands are
/// evaluated and which of them are skipped by short-circuiting. Rewriting
/// `b > a` as `a < b` calls `__lt__` instead of `__gt__`, which can behave
/// differently for user-defined types.
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.9.0", category = Category::Style)]
pub(crate) struct BooleanChainedComparison;

impl AlwaysFixableViolation for BooleanChainedComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Contains chained boolean comparison that can be simplified".to_string()
    }

    fn fix_title(&self) -> String {
        "Use a single compare expression".to_string()
    }
}

/// PLR1716
pub(crate) fn boolean_chained_comparison(checker: &Checker, expr_bool_op: &ExprBoolOp) {
    // early exit for non `and` boolean operations
    if expr_bool_op.op != BoolOp::And {
        return;
    }

    // early exit when not all expressions are compare expressions
    if !expr_bool_op.values.iter().all(Expr::is_compare_expr) {
        return;
    }

    let locator = checker.locator();
    let tokens = checker.tokens();

    // retrieve all compare expressions from boolean expression
    let compare_expressions = expr_bool_op
        .values
        .iter()
        .map(|expr| expr.as_compare_expr().unwrap());

    for (left_compare, right_compare) in compare_expressions.tuple_windows() {
        // Fast path: contiguous forward chain (e.g. `a < b and b < c`, `a < b < c and c < d`).
        if are_compare_expr_simplifiable(left_compare, right_compare)
            && let Some(Expr::Name(left_compare_right)) = left_compare.operands.last()
            && let Expr::Name(right_compare_left) = right_compare.first_operand()
            && left_compare_right.id() == right_compare_left.id()
        {
            let left_paren_count =
                parentheses_iterator(left_compare.into(), Some(expr_bool_op.into()), tokens)
                    .count();

            let right_paren_count =
                parentheses_iterator(right_compare.into(), Some(expr_bool_op.into()), tokens)
                    .count();

            // Create the edit that removes the comparison operator
            // In `a<(b) and ((b))<c`, we need to handle the
            // parentheses when specifying the fix range.
            let left_compare_right_range =
                parenthesized_range(left_compare_right.into(), left_compare.into(), tokens)
                    .unwrap_or(left_compare_right.range());
            let right_compare_left_range =
                parenthesized_range(right_compare_left.into(), right_compare.into(), tokens)
                    .unwrap_or(right_compare_left.range());
            let edit = Edit::range_replacement(
                locator.slice(left_compare_right_range).to_string(),
                TextRange::new(
                    left_compare_right_range.start(),
                    right_compare_left_range.end(),
                ),
            );

            // Balance left and right parentheses
            let fix = match left_paren_count.cmp(&right_paren_count) {
                std::cmp::Ordering::Less => {
                    let balance_parens_edit = Edit::insertion(
                        "(".repeat(right_paren_count - left_paren_count),
                        left_compare.start(),
                    );
                    Fix::safe_edits(edit, [balance_parens_edit])
                }
                std::cmp::Ordering::Equal => Fix::safe_edit(edit),
                std::cmp::Ordering::Greater => {
                    let balance_parens_edit = Edit::insertion(
                        ")".repeat(left_paren_count - right_paren_count),
                        right_compare.end(),
                    );
                    Fix::safe_edits(edit, [balance_parens_edit])
                }
            };

            let mut diagnostic = checker.report_diagnostic(
                BooleanChainedComparison,
                TextRange::new(left_compare.start(), right_compare.end()),
            );

            diagnostic.set_fix(fix);
            continue;
        }

        // Secondary path: reversed condition order or inverted operators
        // (e.g. `b < c and a < b`, `b > a and b < c`, `b < c and b > a`).
        if !is_boolean_chained_comparison_reversed_enabled(checker.settings()) {
            continue;
        }

        let Some((left_l, left_op, left_r)) = left_compare.as_single() else {
            continue;
        };
        let Some((right_l, right_op, right_r)) = right_compare.as_single() else {
            continue;
        };

        let Some((c1_lower, c1_dir, c1_upper)) = to_canonical(left_l, *left_op, left_r) else {
            continue;
        };
        let Some((c2_lower, c2_dir, c2_upper)) = to_canonical(right_l, *right_op, right_r) else {
            continue;
        };

        let both_originally_greater =
            matches!(left_op, CmpOp::Gt | CmpOp::GtE) && matches!(right_op, CmpOp::Gt | CmpOp::GtE);

        // Check if they form a chain via a shared identifier.
        let chain = if let (Expr::Name(c1_upper_name), Expr::Name(c2_lower_name)) =
            (c1_upper, c2_lower)
            && c1_upper_name.id() == c2_lower_name.id()
        {
            // Way 1: `c1` then `c2` in canonical ascending order.
            Some((
                (c1_lower, left_compare),
                c1_dir,
                c1_upper_name,
                c2_dir,
                (c2_upper, right_compare),
            ))
        } else if let (Expr::Name(c2_upper_name), Expr::Name(c1_lower_name)) = (c2_upper, c1_lower)
            && c2_upper_name.id() == c1_lower_name.id()
        {
            // Way 2: `c2` then `c1` in canonical ascending order.
            Some((
                (c2_lower, right_compare),
                c2_dir,
                c2_upper_name,
                c1_dir,
                (c1_upper, left_compare),
            ))
        } else {
            None
        };

        let Some(((lower, lower_compare), dir1, middle, dir2, (upper, upper_compare))) = chain
        else {
            continue;
        };

        // Keep any parentheses around the outer operands, e.g. `(x or y) < b`.
        let lower = locator.slice(
            parenthesized_range(lower.into(), lower_compare.into(), tokens)
                .unwrap_or(lower.range()),
        );
        let upper = locator.slice(
            parenthesized_range(upper.into(), upper_compare.into(), tokens)
                .unwrap_or(upper.range()),
        );

        let replacement = if both_originally_greater {
            format!(
                "{upper} {} {} {} {lower}",
                inv_dir_to_str(dir2),
                middle.id(),
                inv_dir_to_str(dir1),
            )
        } else {
            format!(
                "{lower} {} {} {} {upper}",
                dir_to_str(dir1),
                middle.id(),
                dir_to_str(dir2),
            )
        };

        let left_range = parenthesized_range(left_compare.into(), expr_bool_op.into(), tokens)
            .unwrap_or(left_compare.range());
        let right_range = parenthesized_range(right_compare.into(), expr_bool_op.into(), tokens)
            .unwrap_or(right_compare.range());

        let edit = Edit::range_replacement(
            replacement,
            TextRange::new(left_range.start(), right_range.end()),
        );

        let mut diagnostic = checker.report_diagnostic(
            BooleanChainedComparison,
            TextRange::new(left_compare.start(), right_compare.end()),
        );

        diagnostic.set_fix(Fix::unsafe_edit(edit));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ComparisonDirection {
    Lt,
    LtE,
}

fn to_canonical<'a>(
    left: &'a Expr,
    op: CmpOp,
    right: &'a Expr,
) -> Option<(&'a Expr, ComparisonDirection, &'a Expr)> {
    match op {
        CmpOp::Lt => Some((left, ComparisonDirection::Lt, right)),
        CmpOp::LtE => Some((left, ComparisonDirection::LtE, right)),
        CmpOp::Gt => Some((right, ComparisonDirection::Lt, left)),
        CmpOp::GtE => Some((right, ComparisonDirection::LtE, left)),
        _ => None,
    }
}

fn dir_to_str(dir: ComparisonDirection) -> &'static str {
    match dir {
        ComparisonDirection::Lt => "<",
        ComparisonDirection::LtE => "<=",
    }
}

fn inv_dir_to_str(dir: ComparisonDirection) -> &'static str {
    match dir {
        ComparisonDirection::Lt => ">",
        ComparisonDirection::LtE => ">=",
    }
}

/// Checks whether two compare expressions are simplifiable
fn are_compare_expr_simplifiable(left: &ExprCompare, right: &ExprCompare) -> bool {
    left.ops
        .iter()
        .chain(right.ops.iter())
        .tuple_windows::<(_, _)>()
        .all(|(left_operator, right_operator)| {
            matches!(
                (left_operator, right_operator),
                (CmpOp::Lt | CmpOp::LtE, CmpOp::Lt | CmpOp::LtE)
                    | (CmpOp::Gt | CmpOp::GtE, CmpOp::Gt | CmpOp::GtE)
            )
        })
}
