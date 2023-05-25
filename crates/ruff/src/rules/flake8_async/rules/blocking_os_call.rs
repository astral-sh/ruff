use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

use super::super::helpers::in_async_function;

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

/// ASYNC102
pub(crate) fn blocking_os_call(checker: &mut Checker, expr: &Expr) {
    if in_async_function(checker.semantic_model()) {
        if let Expr::Call(ast::ExprCall { func, .. }) = expr {
            let is_unsafe_os_method = checker
                .semantic_model()
                .resolve_call_path(func)
                .map_or(false, |path| UNSAFE_OS_METHODS.contains(&path.as_slice()));

            if is_unsafe_os_method {
                checker
                    .diagnostics
                    .push(Diagnostic::new(BlockingOsCallInAsyncFunction, func.range()));
            }
        }
    }
}
