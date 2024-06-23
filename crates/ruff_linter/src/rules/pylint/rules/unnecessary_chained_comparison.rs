use std::collections::HashMap;

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};

/// ## What it does
/// Checks for boolean operations such as `a < b and b < c`
/// that can be refactored into a single comparison `a < b < c`.
///
/// ## Why is this bad?
/// A single comparison is semantically clearer and more concise.
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

fn update_bounds<'a>(
    operator: ast::CmpOp,
    id: &'a str,
    node_idx: u32,
    is_left: bool,
    uses: &mut HashMap<&'a str, (u32, u32)>,
) {
    // Store the bounds using bitwise operations. Each bit in these
    // integers represents whether the identifier is involved in a lower or upper bound
    // comparison at a specific node index.
    //
    // For example, if node_idx is 2, then:
    // - `1 << node_idx` produces a bitmask with the 3rd bit set.
    // - Using |=, we set the corresponding bit in the lower or upper bounds integer to 1 without modifying other bits.
    //
    // This efficiently tracks whether the identifier is used in lower or upper bounds
    // comparisons across multiple nodes, allowing us to check for shared bounds later.
    match operator {
        ast::CmpOp::Lt | ast::CmpOp::LtE if is_left => {
            let entry = uses.entry(id).or_default();
            entry.0 |= 1 << node_idx;
        }
        ast::CmpOp::Gt | ast::CmpOp::GtE if is_left => {
            let entry = uses.entry(id).or_default();
            entry.1 |= 1 << node_idx;
        }
        ast::CmpOp::Lt | ast::CmpOp::LtE if !is_left => {
            let entry = uses.entry(id).or_default();
            entry.1 |= 1 << node_idx;
        }
        ast::CmpOp::Gt | ast::CmpOp::GtE if !is_left => {
            let entry = uses.entry(id).or_default();
            entry.0 |= 1 << node_idx;
        }
        _ => {}
    }
}

fn set_lower_upper_bounds<'a>(
    node: &'a ast::ExprCompare,
    uses: &mut HashMap<&'a str, (u32, u32)>,
    node_idx: u32,
) {
    let mut left_operand: &ast::Expr = &node.left;
    for (right_operand, operator) in node.comparators.iter().zip(node.ops.iter()) {
        if let Some(left_name_expr) = left_operand.as_name_expr() {
            update_bounds(*operator, &left_name_expr.id, node_idx, true, uses);
        }

        if let Some(right_name_expr) = right_operand.as_name_expr() {
            update_bounds(*operator, &right_name_expr.id, node_idx, false, uses);
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

    // Use a hashmap to store the lower and upper bounds for each identifier.
    let mut uses: HashMap<&str, (u32, u32)> = HashMap::new();

    let mut node_idx: u32 = 0;
    for expr in values {
        let Some(compare_expr) = expr.as_compare_expr() else {
            continue;
        };
        set_lower_upper_bounds(compare_expr, &mut uses, node_idx);
        node_idx += 1;
    }

    for (lower_bound, upper_bound) in uses.values() {
        let shared_bounds = lower_bound & upper_bound;
        let lower_bound_count = lower_bound.count_ones();
        let upper_bound_count = upper_bound.count_ones();
        let shared_count = shared_bounds.count_ones();

        if shared_count < lower_bound_count && shared_count < upper_bound_count {
            let diagnostic = Diagnostic::new(UnnecessaryChainedComparison, *range);
            checker.diagnostics.push(diagnostic);
            break;
        }
    }
}
