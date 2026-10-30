use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::{Expr, ExprName, Int, UnaryOp};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::typing;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Pedantic)]
pub(crate) struct ListReverseCopy {
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
pub(crate) fn list_assign_reversed(checker: &Checker, assign: &ast::StmtAssign<'_>) {
    let Some([flat::ExprKind::Name(target_expr)]) = assign
        .targets
        .as_array()
        .map(|items| items.map(flat::View::<Expr>::classify))
    else {
        return;
    };

    let Some(reversed_expr) = extract_reversed(assign.value, checker.semantic()) else {
        return;
    };

    if reversed_expr.id() != target_expr.id() {
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

    checker
        .report_diagnostic(
            ListReverseCopy {
                name: target_expr.id().to_string(),
            },
            assign.range,
        )
        .set_fix(Fix::unsafe_edit(Edit::range_replacement(
            format!("{}.reverse()", target_expr.id()),
            assign.range,
        )));
}

/// Recursively removes any `list` wrappers from the expression.
///
/// For example, given `list(list(list([1, 2, 3])))`, this function
/// would return the inner `[1, 2, 3]` expression.
fn peel_lists(expr: flat::View<'_, Expr>) -> flat::View<'_, Expr> {
    let Some(ast::ExprCall {
        func, arguments, ..
    }) = expr
        .as_call_expr()
        .map(flat::View::<ruff_python_ast::ExprCall>::fields)
    else {
        return expr;
    };

    if !arguments.keywords().is_empty() {
        return expr;
    }

    if func.as_name_expr().is_none_or(|name| name.id() != "list") {
        return expr;
    }

    let Some([arg]) = arguments.args().as_array() else {
        return expr;
    };

    peel_lists(arg)
}

/// Given a call to `reversed`, returns the inner argument.
///
/// For example, given `reversed(l)`, this function would return `l`.
fn extract_name_from_reversed<'a>(
    expr: flat::View<'a, Expr>,
    semantic: &SemanticModel,
) -> Option<flat::View<'a, ExprName>> {
    let expr_call = expr.as_call_expr()?;
    let func = expr_call.func();
    let arguments = expr_call.arguments();

    if !arguments.keywords().is_empty() {
        return None;
    }

    let [arg] = arguments.args().as_array()?;

    if !semantic.match_builtin_expr(func, "reversed") {
        return None;
    }

    arg.as_name_expr()
}

/// Given a slice expression, returns the inner argument if it's a reversed slice.
///
/// For example, given `l[::-1]`, this function would return `l`.
fn extract_name_from_sliced_reversed(
    expr: flat::View<'_, Expr>,
) -> Option<flat::View<'_, ExprName>> {
    let expr_subscript = expr.as_subscript_expr()?;
    let value = expr_subscript.value();
    let slice = expr_subscript.slice();
    let expr_slice = slice.as_slice_expr()?;
    let lower = expr_slice.lower();
    let upper = expr_slice.upper();
    let step = expr_slice.step();
    if lower.is_some() || upper.is_some() {
        return None;
    }
    let Some(ast::ExprUnaryOp {
        op: UnaryOp::USub,
        operand,
        ..
    }) = step
        .and_then(flat::View::as_unary_op_expr)
        .map(flat::View::<ruff_python_ast::ExprUnaryOp>::fields)
    else {
        return None;
    };
    if operand
        .as_number_literal_expr()
        .and_then(|num| num.value().as_int())
        .and_then(Int::as_u8)
        .is_none_or(|value| value != 1)
    {
        return None;
    }
    value.as_name_expr()
}

fn extract_reversed<'a>(
    expr: flat::View<'a, Expr>,
    semantic: &SemanticModel,
) -> Option<flat::View<'a, ExprName>> {
    let expr = peel_lists(expr);
    extract_name_from_reversed(expr, semantic).or_else(|| extract_name_from_sliced_reversed(expr))
}
