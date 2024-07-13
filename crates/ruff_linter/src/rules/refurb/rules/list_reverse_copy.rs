use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    Expr, ExprCall, ExprName, ExprSlice, ExprSubscript, ExprUnaryOp, Int, StmtAssign, UnaryOp,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for list reversals that can be performed in-place in lieu of
/// creating a new list.
///
/// ## Why is this bad?
/// When reversing a list, it's more efficient to use the in-place method
/// `.reverse()` instead of creating a new list, if the original list is
/// no longer needed.
///
/// ## Example
/// ```python
/// l = [1, 2, 3]
/// l = reversed(l)
///
/// l = [1, 2, 3]
/// l = list(reversed(l))
///
/// l = [1, 2, 3]
/// l = l[::-1]
/// ```
///
/// Use instead:
/// ```python
/// l = [1, 2, 3]
/// l.reverse()
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as calling `.reverse()` on a list
/// will mutate the list in-place, unlike `reversed`, which creates a new list
/// and leaves the original list unchanged.
///
/// If the list is referenced elsewhere, this could lead to unexpected
/// behavior.
///
/// ## References
/// - [Python documentation: More on Lists](https://docs.python.org/3/tutorial/datastructures.html#more-on-lists)
#[violation]
pub struct ListReverseCopy {
    name: String,
}

impl AlwaysFixableViolation for ListReverseCopy {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ListReverseCopy { name } = self;
        format!("Use of assignment of `reversed` on list `{name}`")
    }

    fn fix_title(&self) -> String {
        let ListReverseCopy { name } = self;
        format!("Replace with `{name}.reverse()`")
    }
}

/// FURB187
pub(crate) fn list_assign_reversed(checker: &mut Checker, assign: &StmtAssign) {
    let [Expr::Name(target_expr)] = assign.targets.as_slice() else {
        return;
    };

    let Some(reversed_expr) = extract_reversed(assign.value.as_ref(), checker.semantic()) else {
        return;
    };

    if reversed_expr.id != target_expr.id {
        return;
    }

    let Some(binding) = checker
        .semantic()
        .only_binding(reversed_expr)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !typing::is_list(binding, checker.semantic()) {
        return;
    }

    checker.diagnostics.push(
        Diagnostic::new(
            ListReverseCopy {
                name: target_expr.id.to_string(),
            },
            assign.range(),
        )
        .with_fix(Fix::unsafe_edit(Edit::range_replacement(
            format!("{}.reverse()", target_expr.id),
            assign.range(),
        ))),
    );
}

/// Recursively removes any `list` wrappers from the expression.
///
/// For example, given `list(list(list([1, 2, 3])))`, this function
/// would return the inner `[1, 2, 3]` expression.
fn peel_lists(expr: &Expr) -> &Expr {
    let Some(ExprCall {
        func, arguments, ..
    }) = expr.as_call_expr()
    else {
        return expr;
    };

    if !arguments.keywords.is_empty() {
        return expr;
    }

    if !func.as_name_expr().is_some_and(|name| name.id == "list") {
        return expr;
    }

    let [arg] = arguments.args.as_ref() else {
        return expr;
    };

    peel_lists(arg)
}

/// Given a call to `reversed`, returns the inner argument.
///
/// For example, given `reversed(l)`, this function would return `l`.
fn extract_name_from_reversed<'a>(
    expr: &'a Expr,
    semantic: &SemanticModel,
) -> Option<&'a ExprName> {
    let ExprCall {
        func, arguments, ..
    } = expr.as_call_expr()?;

    if !arguments.keywords.is_empty() {
        return None;
    }

    let [arg] = arguments.args.as_ref() else {
        return None;
    };

    if !semantic.match_builtin_expr(func, "reversed") {
        return None;
    }

    arg.as_name_expr()
}

/// Given a slice expression, returns the inner argument if it's a reversed slice.
///
/// For example, given `l[::-1]`, this function would return `l`.
fn extract_name_from_sliced_reversed(expr: &Expr) -> Option<&ExprName> {
    let ExprSubscript { value, slice, .. } = expr.as_subscript_expr()?;
    let ExprSlice {
        lower, upper, step, ..
    } = slice.as_slice_expr()?;
    if lower.is_some() || upper.is_some() {
        return None;
    }
    let Some(ExprUnaryOp {
        op: UnaryOp::USub,
        operand,
        ..
    }) = step.as_ref().and_then(|expr| expr.as_unary_op_expr())
    else {
        return None;
    };
    if !operand
        .as_number_literal_expr()
        .and_then(|num| num.value.as_int())
        .and_then(Int::as_u8)
        .is_some_and(|value| value == 1)
    {
        return None;
    };
    value.as_name_expr()
}

fn extract_reversed<'a>(expr: &'a Expr, semantic: &SemanticModel) -> Option<&'a ExprName> {
    let expr = peel_lists(expr);
    extract_name_from_reversed(expr, semantic).or_else(|| extract_name_from_sliced_reversed(expr))
}
