use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    Expr, ExprCall, ExprName, ExprSlice, ExprSubscript, ExprUnaryOp, Int, StmtAssign, UnaryOp,
};
use ruff_python_semantic::analyze::typing;

/// ## What it does
/// Checks for uses of assignment of "reversed" expression on the list to the same binging.
///
/// ## Why is this bad?
///
/// Use of in-place method `.reverse()` is faster and allows to avoid copying the name of variable.
///
/// ## Example
/// ```python
/// l = [1, 2, 3]
/// l = reversed(l)
/// l = list(reversed(l))
/// l = l[::-1]
/// ```
///
/// Use instead:
/// ```python
/// l = [1, 2, 3]
/// l.reverse()
/// l.reverse()
/// l.reverse()
/// ```
///
/// ## References
/// - [Python documentation: More on Lists](https://docs.python.org/3/tutorial/datastructures.html#more-on-lists)
#[violation]
pub struct ListAssignReversed {
    name: String,
}

impl AlwaysFixableViolation for ListAssignReversed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of assignment of `reversed` on list `{}`", self.name)
    }

    fn fix_title(&self) -> String {
        format!("Use `{}.reverse()` instead", self.name)
    }
}

fn extract_name_from_reversed(expr: &Expr) -> Option<&ExprName> {
    let ExprCall {
        func, arguments, ..
    } = expr.as_call_expr()?;
    if !arguments.keywords.is_empty() {
        return None;
    }
    let [arg] = arguments.args.as_ref() else {
        return None;
    };

    func.as_name_expr()
        .is_some_and(|name_expr| name_expr.id == "reversed")
        .then(|| arg.as_name_expr())
        .flatten()
}

fn peel_lists(expr: &Expr) -> &Expr {
    let Some(ExprCall {
        func, arguments, ..
    }) = expr.as_call_expr()
    else {
        return expr;
    };
    if !arguments.keywords.is_empty()
        || func
            .as_name_expr()
            .map_or(true, |expr_name| expr_name.id != "list")
    {
        return expr;
    }
    if let [arg] = arguments.args.as_ref() {
        peel_lists(arg)
    } else {
        expr
    }
}

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
    if operand
        .as_number_literal_expr()
        .and_then(|num| num.value.as_int())
        .and_then(Int::as_u8)
        != Some(1)
    {
        return None;
    };
    value.as_name_expr()
}

fn extract_name_from_general_reversed(expr: &Expr) -> Option<&ExprName> {
    let expr = peel_lists(expr);
    extract_name_from_reversed(expr).or_else(|| extract_name_from_sliced_reversed(expr))
}

// FURB187
pub(crate) fn list_assign_reversed(checker: &mut Checker, assign: &StmtAssign) {
    let [Expr::Name(target_name_expr)] = assign.targets.as_slice() else {
        return;
    };

    let Some(arg_name_expr) = extract_name_from_general_reversed(assign.value.as_ref()) else {
        return;
    };

    if arg_name_expr.id != target_name_expr.id {
        return;
    }

    let Some(binding) = checker
        .semantic()
        .only_binding(arg_name_expr)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !typing::is_list(binding, checker.semantic()) {
        return;
    }

    checker.diagnostics.push(
        Diagnostic::new(
            ListAssignReversed {
                name: target_name_expr.id.to_string(),
            },
            assign.range,
        )
        .with_fix(Fix::safe_edit(Edit::range_replacement(
            format!("{}.reverse()", target_name_expr.id),
            assign.range,
        ))),
    );
}
