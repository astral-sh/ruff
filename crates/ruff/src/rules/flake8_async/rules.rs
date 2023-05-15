use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;

struct ViolatingCalls<'a> {
    members: &'a [&'a [&'a str]],
}

impl<'a> ViolatingCalls<'a> {
    pub(crate) const fn new(members: &'a [&'a [&'a str]]) -> Self {
        Self { members }
    }
}

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
pub struct BlockingHttpCallInsideAsyncDef;

impl Violation for BlockingHttpCallInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call blocking HTTP methods")
    }
}

const BLOCKING_HTTP_CALLS: &[ViolatingCalls] = &[ViolatingCalls::new(&[
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
])];

/// ASY100
pub(crate) fn blocking_http_call_inside_async_def(checker: &mut Checker, expr: &Expr) {
    if checker
        .ctx
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(false)
    {
        if let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node {
            if let Some(call_path) = checker.ctx.resolve_call_path(func) {
                for v_call in BLOCKING_HTTP_CALLS {
                    for member in v_call.members {
                        if call_path.as_slice() == *member {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(BlockingHttpCallInsideAsyncDef, func.range));
                        }
                    }
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
pub struct OpenSleepOrSubprocessInsideAsyncDef;

impl Violation for OpenSleepOrSubprocessInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call `open`, `time.sleep`, or `subprocess` methods")
    }
}

const OPEN_SLEEP_OR_SUBPROCESS_CALL: &[ViolatingCalls] = &[ViolatingCalls::new(&[
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
])];

/// ASY101
pub(crate) fn open_sleep_or_subprocess_inside_async_def(checker: &mut Checker, expr: &Expr) {
    if checker
        .ctx
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(false)
    {
        if let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node {
            if let Some(call_path) = checker.ctx.resolve_call_path(func) {
                for v_call in OPEN_SLEEP_OR_SUBPROCESS_CALL {
                    for member in v_call.members {
                        if call_path.as_slice() == *member {
                            checker.diagnostics.push(Diagnostic::new(
                                OpenSleepOrSubprocessInsideAsyncDef,
                                func.range,
                            ));
                        }
                    }
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
pub struct UnsafeOsMethodInsideAsyncDef;

impl Violation for UnsafeOsMethodInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call synchronous `os` methods")
    }
}

const UNSAFE_OS_METHODS: &[ViolatingCalls] = &[ViolatingCalls::new(&[
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
])];

/// ASY102
pub(crate) fn unsafe_os_method_inside_async_def(checker: &mut Checker, expr: &Expr) {
    if checker
        .ctx
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(false)
    {
        if let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node {
            if let Some(call_path) = checker.ctx.resolve_call_path(func) {
                for v_call in UNSAFE_OS_METHODS {
                    for member in v_call.members {
                        if call_path.as_slice() == *member {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(UnsafeOsMethodInsideAsyncDef, func.range));
                        }
                    }
                }
            }
        }
    }
}
