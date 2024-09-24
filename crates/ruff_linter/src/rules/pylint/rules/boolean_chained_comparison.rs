use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{name::Name, BoolOp, CmpOp, Expr, ExprBoolOp, ExprCompare};
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
    range: TextRange,
    replace_range: TextRange,
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

    // retrieve all compare statements from expression
    let compare_exprs: Vec<&ExprCompare> = expr_bool_op
        .values
        .iter()
        .map(|stmt| stmt.as_compare_expr().unwrap())
        .collect();

    let results: Vec<BooleanChainedComparison> = compare_exprs
        .iter()
        .tuple_windows::<(&&ExprCompare, &&ExprCompare)>()
        .filter(|(left_compare, right_compare)| {
            are_compare_expr_simplifiable(left_compare, right_compare)
        })
        .filter_map(|(left_compare, right_compare)| {
            let Expr::Name(left_compare_right) = left_compare.comparators.first().unwrap() else {
                return None;
            };

            let Expr::Name(ref right_compare_left) = right_compare.left.as_ref() else {
                return None;
            };

            if left_compare_right.id() != right_compare_left.id() {
                return None;
            }

            Some(BooleanChainedComparison {
                variable: left_compare_right.id().clone(),
                range: TextRange::new(left_compare.start(), right_compare.end()),
                replace_range: TextRange::new(left_compare_right.start(), right_compare_left.end()),
            })
        })
        .collect();

    checker
        .diagnostics
        .extend(results.into_iter().map(|result| {
            let range = result.range;
            let edit = Edit::range_replacement(result.variable.to_string(), result.replace_range);
            let mut diagnostic = Diagnostic::new(result, range);
            diagnostic.set_fix(Fix::safe_edit(edit));
            diagnostic
        }));
}

/// Checks whether two compare expressions are simplifiable
fn are_compare_expr_simplifiable(left: &ExprCompare, right: &ExprCompare) -> bool {
    // only allow simplifying simple compare operations
    if left.ops.len() != 1 || right.ops.len() != 1 {
        return false;
    }

    matches!(
        (left.ops.first().unwrap(), right.ops.first().unwrap()),
        (CmpOp::Lt | CmpOp::LtE, CmpOp::Lt | CmpOp::LtE)
            | (CmpOp::Gt | CmpOp::GtE, CmpOp::Gt | CmpOp::GtE)
    )
}
