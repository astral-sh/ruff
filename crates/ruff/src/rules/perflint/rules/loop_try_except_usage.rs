use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::{Ranged, Stmt, StmtTry};
use rustpython_parser::ast::{ExceptHandler, ExceptHandlerExceptHandler};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks to see if a for loop contains a try/except block.
///
/// ## Why is this bad?
/// Try/except blocks can be computationally expensive, especially prior to Python 3.10.
/// Instead, you should refactor your code to put the entire loop into the `try` block.
///
/// ## Example
/// ```python
/// for _ in range(10):
///     try:
///         print("something")
///     except:
///         print("error")
/// ```
///
/// Use instead:
/// ```python
/// try:
///     for _ in range(10):
///         print("something")
/// except:
///     print("error")
/// ```
#[violation]
pub struct LoopTryExceptUsage;

impl Violation for LoopTryExceptUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Try..except blocks can have significant overhead. Avoid using them inside of a loop."
        )
    }
}

/// PERF203
pub(crate) fn loop_try_except_usage(checker: &mut Checker, body: &[Stmt]) {
    body.iter()
        .filter_map(|stmt| match stmt {
            Stmt::Try(StmtTry { handlers, .. }) => {
                if let Some(handler) = handlers.iter().next() {
                    let ExceptHandler::ExceptHandler(ExceptHandlerExceptHandler { .. }) = handler;
                    Some(Diagnostic::new(LoopTryExceptUsage, handler.range()))
                } else {
                    None
                }
            }
            _ => None,
        })
        .for_each(|diagnostic| checker.diagnostics.push(diagnostic));
}
