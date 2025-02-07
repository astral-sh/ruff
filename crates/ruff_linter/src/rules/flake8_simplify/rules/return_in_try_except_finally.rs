use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `return` statements in `try`-`except` and `finally` blocks.
///
/// ## Why is this bad?
/// The `return` statement in a `finally` block will always be executed, even if
/// an exception is raised in the `try` or `except` block. This can lead to
/// unexpected behavior.
///
/// ## Example
/// ```python
/// def squared(n):
///     try:
///         sqr = n**2
///         return sqr
///     except Exception:
///         return "An exception occurred"
///     finally:
///         return -1  # Always returns -1.
/// ```
///
/// Use instead:
/// ```python
/// def squared(n):
///     try:
///         return_value = n**2
///     except Exception:
///         return_value = "An exception occurred"
///     finally:
///         return_value = -1
///     return return_value
/// ```
///
/// ## References
/// - [Python documentation: Defining Clean-up Actions](https://docs.python.org/3/tutorial/errors.html#defining-clean-up-actions)
#[derive(ViolationMetadata)]
pub(crate) struct ReturnInTryExceptFinally;

impl Violation for ReturnInTryExceptFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Don't use `return` in `try`-`except` and `finally`".to_string()
    }
}

fn find_return(stmts: &[Stmt]) -> Option<&Stmt> {
    stmts.iter().find(|stmt| stmt.is_return_stmt())
}

/// SIM107
pub(crate) fn return_in_try_except_finally(
    checker: &Checker,
    body: &[Stmt],
    handlers: &[ExceptHandler],
    finalbody: &[Stmt],
) {
    let try_has_return = find_return(body).is_some();
    let except_has_return = handlers.iter().any(|handler| {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) = handler;
        find_return(body).is_some()
    });

    if try_has_return || except_has_return {
        if let Some(finally_return) = find_return(finalbody) {
            checker.report_diagnostic(Diagnostic::new(
                ReturnInTryExceptFinally,
                finally_return.range(),
            ));
        }
    }
}
