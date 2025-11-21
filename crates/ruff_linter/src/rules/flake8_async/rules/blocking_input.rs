use ruff_python_ast::ExprCall;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not contain blocking usage of input from user.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking input call will block the entire
/// event loop, preventing it from executing other tasks while waiting for user
/// input, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking input call directly, wrap the input call in
/// an executor to execute the blocking call on another thread.
///
/// ## Example
/// ```python
/// async def foo():
///     username = input("Username:")
/// ```
///
/// Use instead:
/// ```python
/// import asyncio
///
///
/// async def foo():
///     loop = asyncio.get_running_loop()
///     username = await loop.run_in_executor(None, input, "Username:")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.12.12")]
pub(crate) struct BlockingInputInAsyncFunction;

impl Violation for BlockingInputInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Blocking call to input() in async context".to_string()
    }
}

/// ASYNC250
pub(crate) fn blocking_input(checker: &Checker, call: &ExprCall) {
    if checker.semantic().in_async_context() {
        if checker.semantic().match_builtin_expr(&call.func, "input") {
            checker.report_diagnostic(BlockingInputInAsyncFunction, call.func.range());
        }
    }
}
