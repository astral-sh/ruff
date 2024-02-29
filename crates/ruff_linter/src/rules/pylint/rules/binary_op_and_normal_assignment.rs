use ast::{Expr, StmtAugAssign};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for normal assignment statements whose target is the same as the
/// left operand of a binary operator, in which cases, augmented assignment
/// could potentially be used instead.
///
/// ## Why is this bad?
/// Augmented assignment operators are more concise to perform a binary
/// operation and assign results back to one of the operands.
///
/// ## Example
/// ```python
/// a = a + 1
/// ```
///
/// Use instead:
/// ```python
/// a += 1
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as being unsafe, in that it could alter semantics
/// of the given Python code in some scenarios.
///
/// For example, the following code using mutable data types as the assignment
/// target
/// ```python
/// a = [1]
/// b = a
/// a = a + [2]
/// assert (a, b) == ([1, 2], [1])
/// ```
///
/// is not the same as
/// ```python
/// a = [1]
/// b = a
/// a += [2]
/// assert (a, b) == ([1, 2], [1, 2])
/// ```
#[violation]
pub struct BinaryOpAndNormalAssignment;

impl Violation for BinaryOpAndNormalAssignment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Normal assignment with left operand of binary operator being the same as the target."
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use augmented assignment instead.".to_string())
    }
}

pub(crate) fn binary_op_and_normal_assignment(
    checker: &mut Checker,
    assign @ ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
) {
    if targets.len() != 1 {
        return;
    }
    let target = targets.first().unwrap();

    let rhs_expr = value
        .as_bin_op_expr()
        .map(|e| (e.left.as_ref(), e.op, e.right.as_ref()));
    if rhs_expr.is_none() {
        return;
    }
    let (left_operand, operator, right_operand) = rhs_expr.unwrap();

    if name_expr(target, left_operand)
        || object_attribute_expr(target, left_operand)
        || index_subscript_expr(target, left_operand)
        || slice_subscript_expr(target, left_operand)
    {
        let diagnostic = Diagnostic::new(BinaryOpAndNormalAssignment, assign.range()).with_fix(
            generate_fix(checker, target, operator, right_operand, assign.range()),
        );
        checker.diagnostics.push(diagnostic);
    }
}

fn name_expr(target: &Expr, left_operand: &Expr) -> bool {
    matches!(
        (
            target.as_name_expr(),
            left_operand.as_name_expr()
        ),
        (
            Some(ast::ExprName {
                id: target_name_id, ..
            }),
            Some(ast::ExprName {
                id: left_name_id, ..
            }),
        ) if target_name_id == left_name_id
    )
}

fn object_attribute_expr(target: &Expr, left_operand: &Expr) -> bool {
    matches!((
            target
                .as_attribute_expr()
                .and_then(|attr| attr.value.as_name_expr())
                .map(|name| &name.id),
            target
                .as_attribute_expr()
                .map(|attr| attr.attr.as_str()),
            left_operand
                .as_attribute_expr()
                .and_then(|attr| attr.value.as_name_expr())
                .map(|name| &name.id),
            left_operand
                .as_attribute_expr()
                .map(|attr| attr.attr.as_str())
        ),
        (
            Some(target_name_id),
            Some(target_attr_id),
            Some(left_name_id),
            Some(left_attr_id)
        )
        if target_name_id == left_name_id && target_attr_id == left_attr_id
    )
}

fn index_subscript_expr(target: &Expr, left_operand: &Expr) -> bool {
    matches!((
            target
                .as_subscript_expr()
                .and_then(|subs| subs.value.as_name_expr())
                .map(|name| &name.id),
            target
                .as_subscript_expr()
                .and_then(|subs| subs.slice.as_number_literal_expr())
                .map(|num| &num.value),
            left_operand
                .as_subscript_expr()
                .and_then(|subs| subs.value.as_name_expr())
                .map(|name| &name.id),
            left_operand
                .as_subscript_expr()
                .and_then(|subs| subs.slice.as_number_literal_expr())
                .map(|num| &num.value),
        ),
        (
            Some(target_name),
            Some(target_subs),
            Some(left_name),
            Some(left_subs)
        )
        if target_name == left_name && target_subs == left_subs
    )
}

fn slice_subscript_expr(target: &Expr, left_operand: &Expr) -> bool {
    match (
        target
            .as_subscript_expr()
            .and_then(|subs| subs.value.as_name_expr())
            .map(|name| &name.id),
        target
            .as_subscript_expr()
            .and_then(|subs| subs.slice.as_slice_expr()),
        left_operand
            .as_subscript_expr()
            .and_then(|subs| subs.value.as_name_expr())
            .map(|name| &name.id),
        left_operand
            .as_subscript_expr()
            .and_then(|subs| subs.slice.as_slice_expr()),
    ) {
        (Some(target_name), Some(target_slice), Some(left_name), Some(left_slice))
            if target_name == left_name =>
        {
            let target_lower = target_slice
                .lower
                .as_ref()
                .and_then(|lower| lower.as_number_literal_expr())
                .map(|num| &num.value);
            let target_upper = target_slice
                .upper
                .as_ref()
                .and_then(|upper| upper.as_number_literal_expr())
                .map(|num| &num.value);
            let left_lower = left_slice
                .lower
                .as_ref()
                .and_then(|lower| lower.as_number_literal_expr())
                .map(|num| &num.value);
            let left_upper = left_slice
                .upper
                .as_ref()
                .and_then(|upper| upper.as_number_literal_expr())
                .map(|num| &num.value);

            target_lower == left_lower && target_upper == left_upper
        }
        _ => false,
    }
}

fn generate_fix(
    checker: &Checker,
    target: &Expr,
    operator: ast::Operator,
    right_operand: &Expr,
    text_range: TextRange,
) -> Fix {
    let new_stmt = ast::Stmt::AugAssign(StmtAugAssign {
        range: TextRange::default(),
        target: Box::new(target.clone()),
        op: operator,
        value: Box::new(right_operand.clone()),
    });
    let content = checker.generator().stmt(&new_stmt);
    Fix::unsafe_edit(Edit::range_replacement(content, text_range))
}
