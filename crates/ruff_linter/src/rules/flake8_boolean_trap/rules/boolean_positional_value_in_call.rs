use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_boolean_trap::helpers::allow_boolean_trap;

/// ## What it does
/// Checks for boolean positional arguments in function calls.
///
/// Some functions are whitelisted by default. To extend the list of allowed calls
/// configure the [`lint.flake8-boolean-trap.extend-allowed-calls`] option.
///
/// ## Why is this bad?
/// Calling a function with boolean positional arguments is confusing as the
/// meaning of the boolean value is not clear to the caller, and to future
/// readers of the code.
///
/// ## Example
///
/// ```python
/// def func(flag: bool) -> None: ...
///
///
/// func(True)
/// ```
///
/// Use instead:
///
/// ```python
/// def func(flag: bool) -> None: ...
///
///
/// func(flag=True)
/// ```
///
/// ## Options
/// - `lint.flake8-boolean-trap.extend-allowed-calls`
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
#[derive(ViolationMetadata)]
pub(crate) struct BooleanPositionalValueInCall;

impl Violation for BooleanPositionalValueInCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Boolean positional value in function call".to_string()
    }
}

pub(crate) fn boolean_positional_value_in_call(checker: &Checker, call: &ast::ExprCall) {
    if allow_boolean_trap(call, checker) {
        return;
    }
    for arg in call
        .arguments
        .args
        .iter()
        .filter(|arg| arg.is_boolean_literal_expr())
    {
        checker.report_diagnostic(Diagnostic::new(BooleanPositionalValueInCall, arg.range()));
    }
}
