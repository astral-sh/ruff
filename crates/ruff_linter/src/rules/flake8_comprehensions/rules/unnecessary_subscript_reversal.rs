use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, UnaryOp};
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
/// ## Example
/// ```python
/// sorted(iterable[::-1])
/// set(iterable[::-1])
/// reversed(iterable[::-1])
/// ```
///
/// Use instead:
/// ```python
/// sorted(iterable)
/// set(iterable)
/// iterable
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessarySubscriptReversal {
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
pub(crate) fn unnecessary_subscript_reversal(checker: &Checker, call: &ast::ExprCall) {
    let Some(first_arg) = call.arguments.args.first() else {
        return;
    };
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
    let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: ast::Number::Int(val),
        ..
    }) = operand.as_ref()
    else {
        return;
    };
    if *val != 1 {
        return;
    }
    let Some(function_name) = checker.semantic().resolve_builtin_symbol(&call.func) else {
        return;
    };
    if !matches!(function_name, "reversed" | "set" | "sorted") {
        return;
    }
    checker.report_diagnostic(Diagnostic::new(
        UnnecessarySubscriptReversal {
            func: function_name.to_string(),
        },
        call.range(),
    ));
}
