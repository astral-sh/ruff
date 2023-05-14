use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;


struct ViolatingCalls<'a> {
    members: &'a [&'a [&'a str]],
}

impl<'a> ViolatingCalls<'a> {
    pub const fn new(members: &'a [&'a [&'a str]]) -> Self {
        Self { members }
    }
}

/// ## What it does
/// Checks that async functions do not contain a blocking HTTP call
///
/// ## Why is this bad?
/// Because blocking an async function makes the asynchronous nature of the function useless and
/// could confuse a reader or user
///
/// ## Example
/// async def foo():
///    urllib.request.urlopen("http://example.com/foo/bar").read()
///
/// Use instead:
/// Many options, but e.g.:
///
/// async def foo():
///    async with aiohttp.ClientSession() as session:
///        async with session.get("http://example.com/foo/bar") as resp:
///            result = await resp.json()
///            print(result)
#[violation]
pub struct BlockingHttpCallInsideAsyncDef;

impl Violation for BlockingHttpCallInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call a blocking HTTP method")
    }
}

// TODO: Complete
const BLOCKING_HTTP_CALLS: &[ViolatingCalls] = &[
    ViolatingCalls::new(
        &[
            &["urllib", "request", "urlopen"],
            &["httpx", "get"],
            &["httpx", "post"],
            &["httpx", "put"],
            &["httpx", "patch"],
            &["httpx", "delete"],
            &["requests", "get"],
            &["requests", "post"],
            &["requests", "delete"],
            &["requests", "patch"],
            &["requests", "put"],
        ]
    )];

/// ASY100
pub fn blocking_http_call_inside_async_def(checker: &mut Checker, expr: &Expr) {
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
        .unwrap_or(false) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if let Some(call_path) = collect_call_path(func) {
                for v_call in BLOCKING_HTTP_CALLS {
                    for member in v_call.members {
                        if call_path.as_slice() == *member {
                            checker.diagnostics.push(Diagnostic::new(BlockingHttpCallInsideAsyncDef, func.range));
                        }
                    }
                }
            }
        }
    }
}

/// ## What it does
/// Checks that async functions do not contain a call to `open`, `time.sleep` or `subprocess`
/// methods
///
/// ## Why is this bad?
/// Calling these functions in an async process can lead to unexpected behaviour
///
/// ## Example
/// async def foo():
///     time.sleep(1000)
///
/// Use instead:
/// def foo():
///     time.sleep(1000)
#[violation]
pub struct OpenSleepOrSubprocessInsideAsyncDef;

impl Violation for OpenSleepOrSubprocessInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Async functions should not contain a call to `open`, `time.sleep` or any \
             subprocess method"
        )
    }
}

const OPEN_SLEEP_OR_SUBPROCESS_CALL: &[ViolatingCalls] = &[
    ViolatingCalls::new(
        &[
            &["open"],
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
        ]
    )];

/// ASY101
pub fn open_sleep_or_subprocess_inside_async_def(checker: &mut Checker, expr: &Expr) {
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
        .unwrap_or(false) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if let Some(call_path) = collect_call_path(func) {
                for v_call in OPEN_SLEEP_OR_SUBPROCESS_CALL {
                    for member in v_call.members {
                        if call_path.as_slice() == *member
                        {
                            checker.diagnostics.push(Diagnostic::new(OpenSleepOrSubprocessInsideAsyncDef, func.range));
                        }
                    }
                }
            }
        }
    }
}

/// ## What it does
/// Checks that async functions do not contain a call to an unsafe `os` method
///
/// ## Why is this bad?
/// Calling unsafe 'os' methods can lead to unpredictable behaviour mid process and/or state
/// changes
///
/// ## Example
/// async def foo():
///     os.popen()
///
/// Use instead:
/// def foo():
///     os.popen()
#[violation]
pub struct UnsafeOsMethodInsideAsyncDef;

impl Violation for UnsafeOsMethodInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not contain a call to unsafe `os` methods")
    }
}

const UNSAFE_OS_METHODS: &[ViolatingCalls] = &[
    ViolatingCalls::new(
        &[
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
            &["os", "system"]
        ]
    )];

/// ASY102
// TODO: Implement
pub fn unsafe_os_method_inside_async_def(checker: &mut Checker, expr: &Expr) {
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
        .unwrap_or(false) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if let Some(call_path) = collect_call_path(func) {
                for v_call in UNSAFE_OS_METHODS {
                    for member in v_call.members {
                        if call_path.as_slice() == *member
                        {
                            checker.diagnostics.push(Diagnostic::new(UnsafeOsMethodInsideAsyncDef, func.range));
                        }
                    }
                }
            }
        }
    }
}
