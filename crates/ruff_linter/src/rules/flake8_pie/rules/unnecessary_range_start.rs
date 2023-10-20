use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AlwaysFixableViolation, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for `range` calls with an unnecessary `start` argument.
///
/// ## Why is this bad?
/// `range(0, x)` is equivalent to `range(x)`, as `0` is the default value for
/// the `start` argument. Omitting the `start` argument makes the code more
/// concise and idiomatic.
///
/// ## Example
/// ```python
/// range(0, 3)
/// ```
///
/// Use instead:
/// ```python
/// range(3)
/// ```
///
/// ## References
/// - [Python documentation: `range`](https://docs.python.org/3/library/stdtypes.html#range)
#[violation]
pub struct UnnecessaryRangeStart;

impl AlwaysFixableViolation for UnnecessaryRangeStart {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `start` argument in `range`")
    }

    fn fix_title(&self) -> String {
        format!("Remove `start` argument")
    }
}

/// PIE808
pub(crate) fn unnecessary_range_start(checker: &mut Checker, call: &ast::ExprCall) {
    // Verify that the call is to the `range` builtin.
    let Expr::Name(ast::ExprName { id, .. }) = call.func.as_ref() else {
        return;
    };
    if id != "range" {
        return;
    };
    if !checker.semantic().is_builtin("range") {
        return;
    };

    // `range` doesn't accept keyword arguments.
    if !call.arguments.keywords.is_empty() {
        return;
    }

    // Verify that the call has exactly two arguments (no `step`).
    let [start, _] = call.arguments.args.as_slice() else {
        return;
    };

    // Verify that the `start` argument is the literal `0`.
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(value),
        ..
    }) = start
    else {
        return;
    };
    if *value != 0 {
        return;
    };

    let mut diagnostic = Diagnostic::new(UnnecessaryRangeStart, start.range());
    diagnostic.try_set_fix(|| {
        remove_argument(
            &start,
            &call.arguments,
            Parentheses::Preserve,
            checker.locator().contents(),
        )
        .map(Fix::safe_edit)
    });
    checker.diagnostics.push(diagnostic);
}
