use std::collections::{HashMap, HashSet};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};

/// ## What it does
/// Checks for boolean operations such as `a < b and b < c`
/// that can be refactored into a single comparison `a < b < c`.
///
/// ## Why is this bad?
/// A single comparison is semantically clearer and reduces the total
/// number of expressions.
///
/// ## Example
/// ```python
/// a = int(input())
/// b = int(input())
/// c = int(input())
/// if a < b and b < c:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// a = int(input())
/// b = int(input())
/// c = int(input())
/// if a < b < c:
///     pass
/// ```

#[violation]
pub struct UnnecessaryChainedComparison;

impl Violation for UnnecessaryChainedComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Simplified chain comparison exists between the operands.")
    }
}

// Bounds struct to store the lower and upper bounds of the operands.
// Each integer in the set represents the id of the operand.
#[derive(Default)]
struct Bounds {
    lower_bound: HashSet<i32>,
    upper_bound: HashSet<i32>,
}

fn set_lower_upper_bounds(node: &ast::ExprCompare, uses: &mut HashMap<String, Bounds>) {
    let mut left_operand: &ast::Expr = &node.left;
    let node_id = node as *const _ as i32;
    for (right_operand, operator) in node.comparators.iter().zip(node.ops.iter()) {
        let Some(left_name_expr) = left_operand.as_name_expr() else {
            continue;
        };

        match operator {
            ast::CmpOp::Lt | ast::CmpOp::LtE => {
                uses.entry(left_name_expr.id.clone())
                    .or_default()
                    .lower_bound
                    .insert(node_id);
            }
            ast::CmpOp::Gt | ast::CmpOp::GtE => {
                uses.entry(left_name_expr.id.clone())
                    .or_default()
                    .upper_bound
                    .insert(node_id);
            }
            _ => {}
        }

        let Some(right_name_expr) = right_operand.as_name_expr() else {
            continue;
        };

        match operator {
            ast::CmpOp::Lt | ast::CmpOp::LtE => {
                uses.entry(right_name_expr.id.clone())
                    .or_default()
                    .upper_bound
                    .insert(node_id);
            }
            ast::CmpOp::Gt | ast::CmpOp::GtE => {
                uses.entry(right_name_expr.id.clone())
                    .or_default()
                    .lower_bound
                    .insert(node_id);
            }
            _ => {}
        }
        left_operand = right_operand;
    }
}

/// PLR1716
pub(crate) fn unnecessary_chained_comparison(checker: &mut Checker, bool_op: &ast::ExprBoolOp) {
    let ast::ExprBoolOp { op, values, range } = bool_op;

    if *op != ast::BoolOp::And || values.len() < 2 {
        return;
    }

    let mut uses: HashMap<String, Bounds> = HashMap::new();
    for expr in values {
        let Some(compare_expr) = expr.as_compare_expr() else {
            return;
        };
        set_lower_upper_bounds(compare_expr, &mut uses);
    }

    for bound in uses.values() {
        let num_shared = bound.lower_bound.intersection(&bound.upper_bound).count();
        if num_shared < bound.lower_bound.len() && num_shared < bound.upper_bound.len() {
            let diagnostic = Diagnostic::new(UnnecessaryChainedComparison, *range);
            checker.diagnostics.push(diagnostic);
        }
    }
}
