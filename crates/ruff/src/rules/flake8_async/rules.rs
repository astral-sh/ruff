use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::context::Context;
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

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

/// ASY100
pub(crate) fn blocking_http_call(checker: &mut Checker, expr: &Expr) {
    if in_async_function(&checker.ctx) {
        if let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node {
            if let Some(call_path) = checker.ctx.resolve_call_path(func) {
                if BLOCKING_HTTP_CALLS.contains(&call_path.as_slice()) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BlockingHttpCallInAsyncFunction, func.range));
                }
            }
        }
    }
}

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

const OPEN_SLEEP_OR_SUBPROCESS_CALL: &[&[&str]] = &[
    &["", "open"],
    &["time", "sleep"],
    &["subprocess", "run"],
    &["subprocess", "Popen"],
    // Deprecated subprocess calls:
    &["subprocess", "call"],
    &["subprocess", "check_call"],
    &["subprocess", "check_output"],
    &["subprocess", "getoutput"],
    &["subprocess", "getstatusoutput"],
    &["os", "wait"],
    &["os", "wait3"],
    &["os", "wait4"],
    &["os", "waitid"],
    &["os", "waitpid"],
];

/// ASY101
pub(crate) fn open_sleep_or_subprocess_call(checker: &mut Checker, expr: &Expr) {
    if in_async_function(&checker.ctx) {
        if let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node {
            if let Some(call_path) = checker.ctx.resolve_call_path(func) {
                if OPEN_SLEEP_OR_SUBPROCESS_CALL.contains(&call_path.as_slice()) {
                    checker.diagnostics.push(Diagnostic::new(
                        OpenSleepOrSubprocessInAsyncFunction,
                        func.range,
                    ));
                }
            }
        }
    }
}

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

const UNSAFE_OS_METHODS: &[&[&str]] = &[
    &["os", "popen"],
    &["os", "posix_spawn"],
    &["os", "posix_spawnp"],
    &["os", "spawnl"],
    &["os", "spawnle"],
    &["os", "spawnlp"],
    &["os", "spawnlpe"],
    &["os", "spawnv"],
    &["os", "spawnve"],
    &["os", "spawnvp"],
    &["os", "spawnvpe"],
    &["os", "system"],
];

/// ASY102
pub(crate) fn blocking_os_call(checker: &mut Checker, expr: &Expr) {
    if in_async_function(&checker.ctx) {
        if let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node {
            if let Some(call_path) = checker.ctx.resolve_call_path(func) {
                if UNSAFE_OS_METHODS.contains(&call_path.as_slice()) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(BlockingOsCallInAsyncFunction, func.range));
                }
            }
        }
    }
}

/// Return `true` if the [`Context`] is inside an async function definition.
fn in_async_function(context: &Context) -> bool {
    context
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(false)
}
