use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    name::Name, parenthesize::parenthesized_range, BoolOp, CmpOp, Expr, ExprBoolOp, ExprCompare,
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
#[violation]
pub struct BooleanChainedComparison {
    variable: Name,
}

impl Violation for BooleanChainedComparison {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Contains chained boolean comparison that can be simplified")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use a single compare expression".to_string())
    }
}

/// PLR1716
pub(crate) fn boolean_chained_comparison(checker: &mut Checker, expr_bool_op: &ExprBoolOp) {
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

    // retrieve all compare statements from expression
    let compare_expressions = expr_bool_op
        .values
        .iter()
        .map(|stmt| stmt.as_compare_expr().unwrap());

    let diagnostics = compare_expressions
        .tuple_windows()
        .filter(|(left_compare, right_compare)| {
            are_compare_expr_simplifiable(left_compare, right_compare)
        })
        .filter_map(|(left_compare, right_compare)| {
            let Expr::Name(left_compare_right) = left_compare.comparators.first()? else {
                return None;
            };

            let Expr::Name(right_compare_left) = &*right_compare.left else {
                return None;
            };

            if left_compare_right.id() != right_compare_left.id() {
                return None;
            }

            let left_has_paren = parenthesized_range(
                left_compare.into(),
                expr_bool_op.into(),
                comment_ranges,
                locator.contents(),
            )
            .is_some();

            let right_has_paren = parenthesized_range(
                right_compare.into(),
                expr_bool_op.into(),
                comment_ranges,
                locator.contents(),
            )
            .is_some();

            // Do not offer a fix if there are any parentheses
            // TODO: We can support a fix here, we just need to be careful to balance the
            // parentheses which requires a more sophisticated edit
            let fix = if left_has_paren || right_has_paren {
                None
            } else {
                let edit = Edit::range_replacement(
                    left_compare_right.id().to_string(),
                    TextRange::new(left_compare_right.start(), right_compare_left.end()),
                );
                Some(Fix::safe_edit(edit))
            };

            let mut diagnostic = Diagnostic::new(
                BooleanChainedComparison {
                    variable: left_compare_right.id().clone(),
                },
                TextRange::new(left_compare.start(), right_compare.end()),
            );

            if let Some(fix) = fix {
                diagnostic.set_fix(fix);
            }

            Some(diagnostic)
        });

    checker.diagnostics.extend(diagnostics);
}

/// Checks whether two compare expressions are simplifiable
fn are_compare_expr_simplifiable(left: &ExprCompare, right: &ExprCompare) -> bool {
    let [left_operator] = &*left.ops else {
        return false;
    };

    let [right_operator] = &*right.ops else {
        return false;
    };

    matches!(
        (left_operator, right_operator),
        (CmpOp::Lt | CmpOp::LtE, CmpOp::Lt | CmpOp::LtE)
            | (CmpOp::Gt | CmpOp::GtE, CmpOp::Gt | CmpOp::GtE)
    )
}
