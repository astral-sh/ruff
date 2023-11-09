use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprCall;

/// ## What it does
/// Detects synchronous HTTP requests in async functions.
///
/// ## Why is this bad?
/// Synchronous HTTP requests in async functions block the event loop.
/// Consider using the relevant implementation for `async/await` code.
///
/// ## Example
/// ```python
/// async def make_request():
///     requests.get("https://astral.sh/")
/// ```
///
/// Use instead:
/// ```python
/// async def make_request():
///     client = httpx.AsyncClient()
///     await client.get("https://astral.sh/")
/// ```
#[violation]
pub struct TrioSyncHTTPCall;

impl Violation for TrioSyncHTTPCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Synchronous HTTP request detected in an async function.")
    }
}

/// TRIO210
pub(crate) fn sync_http_call(checker: &mut Checker, call: &ExprCall) {
    if !checker.semantic().in_async_context() {
        return;
    }

    if checker
        .semantic()
        .resolve_call_path(call.func.as_ref())
        .is_some_and(|path| {
            matches!(
                path.as_slice(),
                [
                    "requests" | "httpx",
                    "get" | "options" | "head" | "post" | "put" | "patch" | "delete"
                ] | ["urllib3", "request"]
                    | ["urllib", "request", "urlopen"]
                    | ["request", "urlopen"]
                    | ["urlopen"]
            )
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(TrioSyncHTTPCall, call.range));
    }
}
