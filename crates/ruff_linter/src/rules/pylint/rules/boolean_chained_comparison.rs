use std::fmt::Display;

use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    BoolOp, CmpOp, Expr, ExprBoolOp,
    token::{parentheses_iterator, parenthesized_range},
};
use ruff_text_size::{Ranged, TextRange};

use crate::{Edit, Fix, FixAvailability, preview::is_chained_comparison_reorder_enabled};
use crate::{Violation, checkers::ast::Checker};

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
#[violation_metadata(stable_since = "0.9.0")]
pub(crate) struct BooleanChainedComparison {
    /// The position of the variable in the left hand side of the `and` or the `or` in the comparison.
    lhs: ComparisonWithVariable,
    /// The position of the variable in the right hand side of the `and` or the `or` in the comparison.
    rhs: ComparisonWithVariable,
}

/// The position of the reused variable in the comparison expression.
///
/// E.g., in `x > 5`, the position of the variable (`x`) is [LEFT].
#[derive(Debug, PartialEq)]
enum VariablePosition {
    Left,
    Right,
}

// Direction of a comparison (i.e. ascending for `<` and descending for `>`).
#[derive(Debug, Eq, PartialEq)]
enum ChainKind {
    Ascending,
    Descending,
}

/// Comparison that uses at least one variable, e.g. `x > 5`.
#[derive(Debug, PartialEq)]
struct ComparisonWithVariable {
    variable_pos: VariablePosition,
    chain_kind: ChainKind,
}
impl Display for ComparisonWithVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.variable_pos, &self.chain_kind) {
            (VariablePosition::Left, ChainKind::Ascending) => write!(f, "x < 7"),
            (VariablePosition::Left, ChainKind::Descending) => write!(f, "x > 3"),
            (VariablePosition::Right, ChainKind::Ascending) => write!(f, "3 < x"),
            (VariablePosition::Right, ChainKind::Descending) => write!(f, "7 > x"),
        }
    }
}

impl Violation for BooleanChainedComparison {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Contains chained boolean comparison that can be simplified".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let Self { lhs, rhs } = self;

        // quick-fix available, so no detailed explanation is needed
        if lhs.variable_pos == VariablePosition::Right && rhs.variable_pos == VariablePosition::Left
        {
            return Some("Use a single compare expression".to_string());
        }

        // we try to re-build the same order of variables in the fix title,
        // so that it's easier to understand how exactly this can be fixed manually.
        Some(format!(
            "Use a single compare expression. For example, '{lhs} and {rhs}' could be simplified to '3 < x < 7'."
        ))
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
        let (Some(left_chain_direction), Some(right_chain_direction)) = (
            comparison_chain_direction(&left_compare.ops),
            comparison_chain_direction(&right_compare.ops),
        ) else {
            // at least one of the comparisons has operations with different directions, e.g. 5 > x < 7
            continue;
        };

        // first case: all comparison operations are pointing in the same direction
        // e.g. <expr> <=? x and x <=? <expr>
        if left_chain_direction == right_chain_direction {
            if is_chained_comparison_reorder_enabled(checker.settings())
                && let Expr::Name(left_compare_left) = &*left_compare.left
                && let Some(Expr::Name(right_compare_right)) = right_compare.comparators.last()
                && left_compare_left.id() == right_compare_right.id()
            {
                // could be simplified by swapping the condition parts
                // e.g. x < 5 and 3 < x -> 3 < x < 5
                // auto-fix would be possible but is not implemented yet
                checker.report_diagnostic(
                    BooleanChainedComparison {
                        lhs: ComparisonWithVariable {
                            variable_pos: VariablePosition::Left,
                            chain_kind: left_chain_direction,
                        },
                        rhs: ComparisonWithVariable {
                            variable_pos: VariablePosition::Right,
                            chain_kind: right_chain_direction,
                        },
                    },
                    TextRange::new(left_compare.start(), right_compare.end()),
                );
                continue;
            }

            let Some(Expr::Name(left_compare_right)) = left_compare.comparators.last() else {
                continue;
            };

            let Expr::Name(right_compare_left) = &*right_compare.left else {
                continue;
            };

            if left_compare_right.id() != right_compare_left.id() {
                continue;
            }

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
                BooleanChainedComparison {
                    lhs: ComparisonWithVariable {
                        variable_pos: VariablePosition::Right,
                        chain_kind: left_chain_direction,
                    },
                    rhs: ComparisonWithVariable {
                        variable_pos: VariablePosition::Left,
                        chain_kind: right_chain_direction,
                    },
                },
                TextRange::new(left_compare.start(), right_compare.end()),
            );

            diagnostic.set_fix(fix);
        } else if is_chained_comparison_reorder_enabled(checker.settings()) {
            // second case: comparison operators point in different directions
            // that makes the fix ambiguous because it's unclear which comparison chain should be reversed (e.g. ">=" -> "<=")
            // e.g. x > 4 and x < 7 -> 4 < x < 7

            let are_left_vars_same_id = if let Expr::Name(left_compare_left) = &*left_compare.left
                && let Expr::Name(right_compare_left) = &*right_compare.left
                && left_compare_left.id() == right_compare_left.id()
            {
                true
            } else {
                false
            };

            if are_left_vars_same_id {
                checker.report_diagnostic(
                    BooleanChainedComparison {
                        lhs: ComparisonWithVariable {
                            variable_pos: VariablePosition::Left,
                            chain_kind: left_chain_direction,
                        },
                        rhs: ComparisonWithVariable {
                            variable_pos: VariablePosition::Left,
                            chain_kind: right_chain_direction,
                        },
                    },
                    TextRange::new(left_compare.start(), right_compare.end()),
                );
                continue;
            }

            let are_right_vars_same_id = if let Some(Expr::Name(left_compare_right)) =
                left_compare.comparators.last()
                && let Some(Expr::Name(right_compare_right)) = right_compare.comparators.last()
                && left_compare_right.id() == right_compare_right.id()
            {
                true
            } else {
                false
            };

            if are_right_vars_same_id {
                checker.report_diagnostic(
                    BooleanChainedComparison {
                        lhs: ComparisonWithVariable {
                            variable_pos: VariablePosition::Right,
                            chain_kind: left_chain_direction,
                        },
                        rhs: ComparisonWithVariable {
                            variable_pos: VariablePosition::Right,
                            chain_kind: right_chain_direction,
                        },
                    },
                    TextRange::new(left_compare.start(), right_compare.end()),
                );
            }
        }
    }
}

/// Checks whether all operations are comparisons and either all comparison operations in the iterator are ascending or all are descending.
fn comparison_chain_direction(ops: &[CmpOp]) -> Option<ChainKind> {
    if ops.iter().all(|op| matches!(op, CmpOp::Lt | CmpOp::LtE)) {
        return Some(ChainKind::Ascending);
    }

    if ops.iter().all(|op| matches!(op, CmpOp::Gt | CmpOp::GtE)) {
        return Some(ChainKind::Descending);
    }

    None
}
