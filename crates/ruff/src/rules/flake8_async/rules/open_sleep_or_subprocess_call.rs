use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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

/// ASYNC101
pub(crate) fn open_sleep_or_subprocess_call(checker: &mut Checker, expr: &Expr) {
    if checker.semantic().in_async_context() {
        if let Expr::Call(ast::ExprCall { func, .. }) = expr {
            let is_open_sleep_or_subprocess_call = checker
                .semantic()
                .resolve_call_path(func)
                .map_or(false, |path| {
                    OPEN_SLEEP_OR_SUBPROCESS_CALL.contains(&path.as_slice())
                });

            if is_open_sleep_or_subprocess_call {
                checker.diagnostics.push(Diagnostic::new(
                    OpenSleepOrSubprocessInAsyncFunction,
                    func.range(),
                ));
            }
        }
    }
}
