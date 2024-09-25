use itertools::Itertools;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
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
}

impl AlwaysFixableViolation for BooleanChainedComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Contains chained boolean comparison that can be simplified")
    }

    fn fix_title(&self) -> String {
        "Use a single compare expression".to_string()
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

            let edit = Edit::range_replacement(
                left_compare_right.id().to_string(),
                TextRange::new(left_compare_right.start(), right_compare_left.end()),
            );

            Some(
                Diagnostic::new(
                    BooleanChainedComparison {
                        variable: left_compare_right.id().clone(),
                    },
                    TextRange::new(left_compare.start(), right_compare.end()),
                )
                .with_fix(Fix::safe_edit(edit)),
            )
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
