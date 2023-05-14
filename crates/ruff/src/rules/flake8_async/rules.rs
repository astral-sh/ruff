use rustpython_parser::ast::{self, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// TDB
///
/// ## Why is this bad?
/// TDB
///
/// ## Example
/// ```python
/// async def f():
///     httpx.get("https://example.com")
/// ```
///
/// Use instead:
/// ```python
/// async def f():
///     async with httpx.AsyncClient() as client:
///         await client.get('https://www.example.com/')
/// ```
#[violation]
pub struct SyncHttpCallInAsyncFunction;

impl Violation for SyncHttpCallInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Sync HTTP call in async function, should use httpx.AsyncClient")
    }
}

#[violation]
pub struct BlockingSyncCallInAsyncFunction;

impl Violation for BlockingSyncCallInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Blocking sync call in async function, use framework equivalent")
    }
}

#[violation]
pub struct SyncProcessCallInAsyncFunction;

impl Violation for SyncProcessCallInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Sync process call in async function, use framework equivalent")
    }
}

const HTTP_PACKAGES: [&str; 2] = ["httpx", "requests"];
const HTTP_METHODS: [&str; 9] = [
    "get", "options", "head", "post", "put", "patch", "delete", "request", "send",
];
const TIME_METHODS: [&str; 1] = ["sleep"];
const SUBPROCESS_METHODS: [&str; 7] = [
    "run",
    "Popen",
    // deprecated methods
    "call",
    "check_call",
    "check_output",
    "getoutput",
    "getstatusoutput",
];
const OS_PROCESS_METHODS: [&str; 12] = [
    "popen",
    "posix_spawn",
    "posix_spawnp",
    "spawnl",
    "spawnle",
    "spawnlp",
    "spawnlpe",
    "spawnv",
    "spawnve",
    "spawnvp",
    "spawnvpe",
    "system",
];
const OS_WAIT_METHODS: [&str; 5] = ["wait", "wait3", "wait4", "waitid", "waitpid"];

pub(crate) fn check_sync_in_async(checker: &mut Checker, body: &[Stmt]) {
    for stmt in body {
        if let StmtKind::Expr(ast::StmtExpr { value }) = &stmt.node {
            if let ExprKind::Call(ast::ExprCall { func, .. }) = &value.node {
                if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
                    if "open" == id.as_str() {
                        let diagnostic =
                            Diagnostic::new(BlockingSyncCallInAsyncFunction, stmt.range());
                        checker.diagnostics.push(diagnostic);
                    }
                } else if let ExprKind::Attribute(ast::ExprAttribute { value, attr, .. }) =
                    &func.node
                {
                    if let ExprKind::Name(ast::ExprName { id, .. }) = &value.node {
                        let module = id.as_str();
                        let method = attr.as_str();
                        let range = stmt.range();

                        if HTTP_PACKAGES.contains(&module) && HTTP_METHODS.contains(&method) {
                            let diagnostic = Diagnostic::new(SyncHttpCallInAsyncFunction, range);
                            checker.diagnostics.push(diagnostic);
                        } else if ("time" == module && TIME_METHODS.contains(&method))
                            || ("subprocess" == module && SUBPROCESS_METHODS.contains(&method))
                        {
                            let diagnostic =
                                Diagnostic::new(BlockingSyncCallInAsyncFunction, range);
                            checker.diagnostics.push(diagnostic);
                        } else if "os" == module {
                            if OS_WAIT_METHODS.contains(&method) {
                                let diagnostic =
                                    Diagnostic::new(BlockingSyncCallInAsyncFunction, range);
                                checker.diagnostics.push(diagnostic);
                            } else if OS_PROCESS_METHODS.contains(&method) {
                                let diagnostic =
                                    Diagnostic::new(SyncProcessCallInAsyncFunction, range);
                                checker.diagnostics.push(diagnostic);
                            }
                        }
                    }
                }
            }
        }
    }
}
