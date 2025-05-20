use itertools::Itertools;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    BoolOp, CmpOp, Expr, ExprBoolOp, ExprCompare,
    parenthesize::{parentheses_iterator, parenthesized_range},
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
#[derive(ViolationMetadata)]
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
    let comment_ranges = checker.comment_ranges();

    // retrieve all compare expressions from boolean expression
    let compare_expressions = expr_bool_op
        .values
        .iter()
        .map(|expr| expr.as_compare_expr().unwrap());

    let diagnostics = compare_expressions
        .tuple_windows()
        .filter(|(left_compare, right_compare)| {
            are_compare_expr_simplifiable(left_compare, right_compare)
        })
        .filter_map(|(left_compare, right_compare)| {
            let Expr::Name(left_compare_right) = left_compare.comparators.last()? else {
                return None;
            };

            let Expr::Name(right_compare_left) = &*right_compare.left else {
                return None;
            };

            if left_compare_right.id() != right_compare_left.id() {
                return None;
            }

            let left_paren_count = parentheses_iterator(
                left_compare.into(),
                Some(expr_bool_op.into()),
                comment_ranges,
                locator.contents(),
            )
            .count();

            let right_paren_count = parentheses_iterator(
                right_compare.into(),
                Some(expr_bool_op.into()),
                comment_ranges,
                locator.contents(),
            )
            .count();

            // Create the edit that removes the comparison operator

            // In `a<(b) and ((b))<c`, we need to handle the
            // parentheses when specifying the fix range.
            let left_compare_right_range = parenthesized_range(
                left_compare_right.into(),
                left_compare.into(),
                comment_ranges,
                locator.contents(),
            )
            .unwrap_or(left_compare_right.range());
            let right_compare_left_range = parenthesized_range(
                right_compare_left.into(),
                right_compare.into(),
                comment_ranges,
                locator.contents(),
            )
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

            let mut diagnostic = Diagnostic::new(
                BooleanChainedComparison,
                TextRange::new(left_compare.start(), right_compare.end()),
            );

            diagnostic.set_fix(fix);

            Some(diagnostic)
        });

    for diagnostic in diagnostics {
        checker.report_diagnostic(diagnostic);
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
