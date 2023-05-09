use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `return` statements in `try`/`except` and `finally` blocks.
///
/// ## Why is this bad?
/// The `return` statement in `finally` blocks will always be executed, even if
/// an exception is raised in the `try` or `except` blocks. This can lead to
/// unexpected behavior.
///
/// ## Example
/// ```python
/// def squared(n):  # always returns -1
///     try:
///         sqr = n**2
///         return sqr
///     except Exception:
///         return "An exception occurred"
///     finally:
///         return -1
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
/// - [Python documentation](https://docs.python.org/3/tutorial/errors.html#defining-clean-up-actions)
#[violation]
pub struct ReturnInTryExceptFinally;

impl Violation for ReturnInTryExceptFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't use `return` in `try`/`except` and `finally`")
    }
}

fn find_return(stmts: &[rustpython_parser::ast::Stmt]) -> Option<&Stmt> {
    stmts
        .iter()
        .find(|stmt| matches!(stmt.node, StmtKind::Return { .. }))
}

/// SIM107
pub fn return_in_try_except_finally(
    checker: &mut Checker,
    body: &[Stmt],
    handlers: &[Excepthandler],
    finalbody: &[Stmt],
) {
    let try_has_return = find_return(body).is_some();
    let except_has_return = handlers.iter().any(|handler| {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        find_return(body).is_some()
    });

    if let Some(finally_return) = find_return(finalbody) {
        if try_has_return || except_has_return {
            checker.diagnostics.push(Diagnostic::new(
                ReturnInTryExceptFinally,
                finally_return.range(),
            ));
        }
    }
}
