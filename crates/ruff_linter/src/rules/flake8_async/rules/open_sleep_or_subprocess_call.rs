use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not contain calls to `time.sleep`, or `subprocess` methods.
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
        format!("Async functions should not call `time.sleep`, or `subprocess` methods")
    }
}

/// ASYNC220
pub(crate) fn open_sleep_or_subprocess_call(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().in_async_context() {
        return;
    }

    if is_sleep_or_subprocess_call(&call.func, checker.semantic()) {
        checker.diagnostics.push(Diagnostic::new(
            OpenSleepOrSubprocessInAsyncFunction,
            call.func.range(),
        ));
    }
}

/// Returns `true` if the expression resolves to a blocking call, like `time.sleep` or
/// `subprocess.run`.
fn is_sleep_or_subprocess_call(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["time", "sleep"]
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
        })
}
