use ruff_python_ast::ExprCall;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not contain calls to blocking synchronous
/// process calls via the `os` module.
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
///     os.popen()
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     os.popen()
/// ```
#[violation]
pub struct BlockingOsCallInAsyncFunction;

impl Violation for BlockingOsCallInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call synchronous `os` methods")
    }
}

/// ASYNC102
pub(crate) fn blocking_os_call(checker: &mut Checker, call: &ExprCall) {
    if checker.semantic().seen_module(Modules::OS) {
        if checker.semantic().in_async_context() {
            if checker
                .semantic()
                .resolve_call_path(call.func.as_ref())
                .as_ref()
                .is_some_and(is_unsafe_os_method)
            {
                checker.diagnostics.push(Diagnostic::new(
                    BlockingOsCallInAsyncFunction,
                    call.func.range(),
                ));
            }
        }
    }
}

fn is_unsafe_os_method(call_path: &CallPath) -> bool {
    matches!(
        call_path.as_slice(),
        [
            "os",
            "popen"
                | "posix_spawn"
                | "posix_spawnp"
                | "spawnl"
                | "spawnle"
                | "spawnlp"
                | "spawnlpe"
                | "spawnv"
                | "spawnve"
                | "spawnvp"
                | "spawnvpe"
                | "system"
        ]
    )
}
