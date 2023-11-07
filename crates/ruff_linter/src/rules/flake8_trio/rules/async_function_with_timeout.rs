use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Parameters;

/// ## What it does
/// Checks for asynchronous functions that have a timeout argument.
///
/// ## Why is this bad?
/// It is preferable to use the special functions that Trio
/// provides to handle timeouts rather than implement them manually.
///
/// ## Example
/// ```python
/// async def func():
///     await long_running_task(timeout=2)
/// ```
///
/// Use instead:
/// ```python
/// async def func():
///     with trio.fail_after(2):
///         await long_running_task()
/// ```
#[violation]
pub struct TrioAsyncFunctionWithTimeout;

impl Violation for TrioAsyncFunctionWithTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async function definition with a `timeout` parameter - use `trio.[fail/move_on]_[after/at]` instead")
    }
}

pub(crate) fn async_function_with_timeout(
    checker: &mut Checker,
    parameters: &Parameters,
    is_async: bool,
) {
    if !is_async {
        return;
    }

    if let Some(timeout_argument) = parameters
        .args
        .iter()
        .find(|argument| argument.parameter.name.eq("timeout"))
    {
        checker.diagnostics.push(Diagnostic::new(
            TrioAsyncFunctionWithTimeout,
            timeout_argument.range,
        ));
    }
}
