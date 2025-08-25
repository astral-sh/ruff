use ruff_python_ast::ExprCall;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not use blocking httpx clients.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking HTTP call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// HTTP response, negating the benefits of asynchronous programming.
///
/// Instead of using the blocking `httpx` client, use the asynchronous client.
///
/// ## Example
/// ```python
/// import httpx
///
///
/// async def fetch():
///     client = httpx.Client()
///     response = client.get(...)
/// ```
///
/// Use instead:
/// ```python
/// import httpx
///
///
/// async def fetch():
///     async with httpx.AsyncClient() as client:
///         response = await client.get(...)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BlockingHttpCallHttpxInAsyncFunction;

impl Violation for BlockingHttpCallHttpxInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not use blocking httpx clients".to_string()
    }
}

fn is_blocking_httpx_client(qualified_name: &QualifiedName) -> bool {
    matches!(qualified_name.segments(), ["httpx", "Client",])
}

/// ASYNC212
pub(crate) fn blocking_http_call_httpx(checker: &Checker, call: &ExprCall) {
    if checker.semantic().in_async_context() {
        if checker
            .semantic()
            .resolve_qualified_name(call.func.as_ref())
            .as_ref()
            .is_some_and(is_blocking_httpx_client)
        {
            checker.report_diagnostic(BlockingHttpCallHttpxInAsyncFunction, call.func.range());
        }
    }
}
