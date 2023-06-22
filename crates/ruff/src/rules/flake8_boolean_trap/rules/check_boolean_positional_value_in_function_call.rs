use rustpython_parser::ast::Expr;

use ruff_diagnostics::Violation;

use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::flake8_boolean_trap::helpers::{add_if_boolean, allow_boolean_trap};

/// ## What it does
/// Checks for boolean positional arguments in function calls.
///
/// ## Why is this bad?
/// Calling a function with boolean positional arguments is confusing as the
/// meaning of the boolean value is not clear to the caller, and to future
/// readers of the code.
///
/// ## Example
/// ```python
/// def foo(flag: bool) -> None:
///     ...
///
///
/// foo(True)
/// ```
///
/// Use instead:
/// ```python
/// def foo(flag: bool) -> None:
///     ...
///
///
/// foo(flag=True)
/// ```
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
#[violation]
pub struct BooleanPositionalValueInFunctionCall;

impl Violation for BooleanPositionalValueInFunctionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional value in function call")
    }
}

pub(crate) fn check_boolean_positional_value_in_function_call(
    checker: &mut Checker,
    args: &[Expr],
    func: &Expr,
) {
    if allow_boolean_trap(func) {
        return;
    }
    for arg in args {
        add_if_boolean(checker, arg, BooleanPositionalValueInFunctionCall.into());
    }
}
