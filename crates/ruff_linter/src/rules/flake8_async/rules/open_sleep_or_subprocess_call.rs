use ruff_python_ast::ExprCall;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not contain calls to `open`, `time.sleep`,
/// or `subprocess` methods.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// call to complete, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking call, use an equivalent asynchronous library
/// or function.
///
/// ## Example
/// ```python
/// async def foo():
///     time.sleep(1000)
/// ```
///
/// Use instead:
/// ```python
/// async def foo():
///     await asyncio.sleep(1000)
/// ```
#[violation]
pub struct OpenSleepOrSubprocessInAsyncFunction;

impl Violation for OpenSleepOrSubprocessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call `open`, `time.sleep`, or `subprocess` methods")
    }
}

/// ASYNC101
pub(crate) fn open_sleep_or_subprocess_call(checker: &mut Checker, call: &ExprCall) {
    if checker.semantic().in_async_context() {
        if checker
            .semantic()
            .resolve_call_path(call.func.as_ref())
            .as_ref()
            .is_some_and(is_open_sleep_or_subprocess_call)
        {
            checker.diagnostics.push(Diagnostic::new(
                OpenSleepOrSubprocessInAsyncFunction,
                call.func.range(),
            ));
        }
    }
}

fn is_open_sleep_or_subprocess_call(call_path: &CallPath) -> bool {
    matches!(
        call_path.as_slice(),
        ["", "open"]
            | ["time", "sleep"]
            | [
                "subprocess",
                "run"
                    | "Popen"
                    | "call"
                    | "check_call"
                    | "check_output"
                    | "getoutput"
                    | "getstatusoutput"
            ]
            | ["os", "wait" | "wait3" | "wait4" | "waitid" | "waitpid"]
    )
}
