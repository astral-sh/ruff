use num_bigint::BigInt;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Unaryop};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for unnecessary subscript reversal of iterable.
///
/// ## Why is it bad?
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
    pub func: String,
}

impl Violation for UnnecessarySubscriptReversal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessarySubscriptReversal { func } = self;
        format!("Unnecessary subscript reversal of iterable within `{func}()`")
    }
}

/// C415
pub fn unnecessary_subscript_reversal(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(first_arg) = args.first() else {
        return;
    };
    let Some(id) = helpers::expr_name(func) else {
        return;
    };
    if !(id == "set" || id == "sorted" || id == "reversed") {
        return;
    }
    if !checker.ctx.is_builtin(id) {
        return;
    }
    let ExprKind::Subscript { slice, .. } = &first_arg.node else {
        return;
    };
    let ExprKind::Slice { lower, upper, step } = &slice.node else {
            return;
        };
    if lower.is_some() || upper.is_some() {
        return;
    }
    let Some(step) = step.as_ref() else {
        return;
    };
    let ExprKind::UnaryOp {
        op: Unaryop::USub,
        operand,
    } = &step.node else {
        return;
    };
    let ExprKind::Constant {
        value: Constant::Int(val),
        ..
    } = &operand.node else {
        return;
    };
    if *val != BigInt::from(1) {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        UnnecessarySubscriptReversal {
            func: id.to_string(),
        },
        Range::from(expr),
    ));
}
