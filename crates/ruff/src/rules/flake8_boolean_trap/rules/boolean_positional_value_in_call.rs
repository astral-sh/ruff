use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_boolean_trap::helpers::{allow_boolean_trap, is_boolean};

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
/// def func(flag: bool) -> None:
///     ...
///
///
/// func(True)
/// ```
///
/// Use instead:
/// ```python
/// def func(flag: bool) -> None:
///     ...
///
///
/// func(flag=True)
/// ```
///
/// ## References
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
/// - [_How to Avoid “The Boolean Trap”_ by Adam Johnson](https://adamj.eu/tech/2021/07/10/python-type-hints-how-to-avoid-the-boolean-trap/)
#[violation]
pub struct BooleanPositionalValueInCall;

impl Violation for BooleanPositionalValueInCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Boolean positional value in function call")
    }
}

pub(crate) fn boolean_positional_value_in_call(checker: &mut Checker, args: &[Expr], func: &Expr) {
    if allow_boolean_trap(func) {
        return;
    }
    for arg in args.iter().filter(|arg| is_boolean(arg)) {
        checker
            .diagnostics
            .push(Diagnostic::new(BooleanPositionalValueInCall, arg.range()));
    }
}
