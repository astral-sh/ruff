use num_bigint::BigInt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, UnaryOp};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary subscript reversal of iterable.
///
/// ## Why is this bad?
/// It's unnecessary to reverse the order of an iterable when passing it
/// into `reversed()`, `set()` or `sorted()` functions as they will change
/// the order of the elements again.
///
/// ## Examples
/// ```python
/// reversed(iterable[::-1])
/// set(iterable[::-1])
/// sorted(iterable)[::-1]
/// ```
///
/// Use instead:
/// ```python
/// reversed(iterable)
/// set(iterable)
/// sorted(iterable, reverse=True)
/// ```
#[violation]
pub struct UnnecessarySubscriptReversal {
    func: String,
}

impl Violation for UnnecessarySubscriptReversal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessarySubscriptReversal { func } = self;
        format!("Unnecessary subscript reversal of iterable within `{func}()`")
    }
}

/// C415
pub(crate) fn unnecessary_subscript_reversal(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(first_arg) = args.first() else {
        return;
    };
    let Some(func) = func.as_name_expr() else {
        return;
    };
    if !matches!(func.id.as_str(), "reversed" | "set" | "sorted") {
        return;
    }
    if !checker.semantic().is_builtin(&func.id) {
        return;
    }
    let Expr::Subscript(ast::ExprSubscript { slice, .. }) = first_arg else {
        return;
    };
    let Expr::Slice(ast::ExprSlice {
        lower,
        upper,
        step,
        range: _,
    }) = slice.as_ref()
    else {
        return;
    };
    if lower.is_some() || upper.is_some() {
        return;
    }
    let Some(step) = step.as_ref() else {
        return;
    };
    let Expr::UnaryOp(ast::ExprUnaryOp {
        op: UnaryOp::USub,
        operand,
        range: _,
    }) = step.as_ref()
    else {
        return;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(val),
        ..
    }) = operand.as_ref()
    else {
        return;
    };
    if *val != BigInt::from(1) {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        UnnecessarySubscriptReversal {
            func: func.id.to_string(),
        },
        expr.range(),
    ));
}
