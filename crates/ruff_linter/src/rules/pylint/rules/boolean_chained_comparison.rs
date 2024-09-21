use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{name::Name, CmpOp, Expr, ExprBoolOp, ExprCompare};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for chained boolean operations that can be simplified.
///
/// ## Why is this bad?
/// Refactoring the code will improve readability for these chained boolean operations.
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
        format!("TODO")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("TODO"))
    }
}

fn iter_all_eq<T: PartialEq>(iter: impl IntoIterator<Item = T>) -> Option<T> {
    let mut iter = iter.into_iter();
    let first = iter.next()?;
    iter.all(|elem| elem == first).then_some(first)
}

#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
enum CompareGrouping {
    Less,
    Greater,
}

/// PLR1716
pub(crate) fn boolean_chained_comparison(checker: &mut Checker, expr_bool_op: &ExprBoolOp) {
    // dbg!(expr_bool_op);

    // early out for boolean expression without multiple compare values
    if expr_bool_op.values.len() == 1 {
        return;
    }

    // early exit when not all expressions are compare expressions
    // TODO: check if this can even happen?
    if !expr_bool_op.values.iter().all(Expr::is_compare_expr) {
        return;
    }

    // retrieve all compare statements from expression
    let compare_exprs: Vec<&ExprCompare> = expr_bool_op
        .values
        .iter()
        .map(|stmt| stmt.as_compare_expr().unwrap())
        .collect();

    // TODO: maybe this should be done per tuple pair instead
    if !are_compare_expr_simplifiable(&compare_exprs) {
        return;
    }

    let results: Vec<Result<(), BooleanChainedComparison>> = compare_exprs
        .iter()
        .tuple_windows::<(&&ExprCompare, &&ExprCompare)>()
        .map(|(left_compare, right_compare)| {
            let Expr::Name(left_compare_right) = left_compare.comparators.first().unwrap() else {
                return Ok(());
            };

            let Expr::Name(ref right_compare_left) = right_compare.left.as_ref() else {
                return Ok(());
            };

            if left_compare_right.id() != right_compare_left.id() {
                return Ok(());
            }

            Err(BooleanChainedComparison {
                variable: left_compare_right.id().clone(),
                range: TextRange::new(left_compare.range.start(), right_compare.range.end()),
                replace_range: TextRange::new(
                    left_compare_right.range.start(),
                    right_compare_left.range.end(),
                ),
            })
        })
        .collect();

    let results: Vec<BooleanChainedComparison> = results
        .into_iter()
        .filter(Result::is_err)
        .map(|result| result.err().unwrap())
        .collect();

    checker.diagnostics.extend(
        results
            .into_iter()
            .map(|result| {
                let variable = result.variable.clone();
                let range = result.range;
                let replace_range = result.replace_range;
                let mut diagnostic = Diagnostic::new(result, range);
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    variable.to_string(),
                    replace_range,
                )));
                diagnostic
            })
            .collect::<Vec<Diagnostic>>(),
    );
}

fn are_compare_expr_simplifiable(compare_exprs: &[&ExprCompare]) -> bool {
    // map compare expressions to compare groups, to be able to check whether grouping is allowed
    let compare_groupings: Vec<CompareGrouping> = compare_exprs
        .iter()
        .filter_map(|compare_expr| compare_expr.ops.first())
        .filter_map(|compare_op| match compare_op {
            CmpOp::Lt | CmpOp::LtE => Some(CompareGrouping::Less),
            CmpOp::Gt | CmpOp::GtE => Some(CompareGrouping::Greater),
            _ => None,
        })
        .collect();

    iter_all_eq(compare_groupings).is_some()
}
