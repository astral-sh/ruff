use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not contain blocking HTTP calls.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking HTTP call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// HTTP response, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking HTTP call, use an asynchronous HTTP client
/// library such as `aiohttp` or `httpx`.
///
/// ## Example
/// ```python
/// async def fetch():
///     urllib.request.urlopen("https://example.com/foo/bar").read()
/// ```
///
/// Use instead:
/// ```python
/// async def fetch():
///     async with aiohttp.ClientSession() as session:
///         async with session.get("https://example.com/foo/bar") as resp:
///             ...
/// ```
#[violation]
pub struct BlockingHttpCallInAsyncFunction;

impl Violation for BlockingHttpCallInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call blocking HTTP methods")
    }
}

const BLOCKING_HTTP_CALLS: &[&[&str]] = &[
    &["urllib", "request", "urlopen"],
    &["httpx", "get"],
    &["httpx", "post"],
    &["httpx", "delete"],
    &["httpx", "patch"],
    &["httpx", "put"],
    &["httpx", "head"],
    &["httpx", "connect"],
    &["httpx", "options"],
    &["httpx", "trace"],
    &["requests", "get"],
    &["requests", "post"],
    &["requests", "delete"],
    &["requests", "patch"],
    &["requests", "put"],
    &["requests", "head"],
    &["requests", "connect"],
    &["requests", "options"],
    &["requests", "trace"],
];

/// ASYNC100
pub(crate) fn blocking_http_call(checker: &mut Checker, expr: &Expr) {
    if checker.semantic_model().in_async_context() {
        if let Expr::Call(ast::ExprCall { func, .. }) = expr {
            let call_path = checker.semantic_model().resolve_call_path(func);
            let is_blocking =
                call_path.map_or(false, |path| BLOCKING_HTTP_CALLS.contains(&path.as_slice()));

            if is_blocking {
                checker.diagnostics.push(Diagnostic::new(
                    BlockingHttpCallInAsyncFunction,
                    func.range(),
                ));
            }
        }
    }
}
