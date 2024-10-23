use ruff_python_ast::ExprCall;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not call `time.sleep`.
///
/// ## Why is this bad?
/// Blocking an async function via a `time.sleep` call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// `time.sleep`, negating the benefits of asynchronous programming.
///
/// Instead of `time.sleep`, use `asyncio.sleep`.
///
/// ## Example
/// ```python
/// async def fetch():
///     time.sleep(1)
/// ```
///
/// Use instead:
/// ```python
/// async def fetch():
///     await asyncio.sleep(1)
/// ```
#[violation]
pub struct BlockingSleepInAsyncFunction;

impl Violation for BlockingSleepInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call `time.sleep`")
    }
}

fn is_blocking_sleep(qualified_name: &QualifiedName) -> bool {
    matches!(qualified_name.segments(), ["time", "sleep"])
}

/// ASYNC251
pub(crate) fn blocking_sleep(checker: &mut Checker, call: &ExprCall) {
    if checker.semantic().in_async_context() {
        if checker
            .semantic()
            .resolve_qualified_name(call.func.as_ref())
            .as_ref()
            .is_some_and(is_blocking_sleep)
        {
            checker.diagnostics.push(Diagnostic::new(
                BlockingSleepInAsyncFunction,
                call.func.range(),
            ));
        }
    }
}
