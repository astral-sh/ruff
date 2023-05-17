use rustpython_parser::ast::Excepthandler::ExceptHandler;
use rustpython_parser::ast::{self, Excepthandler, ExcepthandlerExceptHandler, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for immediate uses of `raise` within exception handlers.
///
/// ## Why is this bad?
/// Capturing an exception, only to immediately reraise it, has no effect.
/// Instead, remove the error-handling code and let the exception propagate
/// upwards without the unnecessary `try`-`except` block.
///
/// ## Example
/// ```python
/// def foo():
///     try:
///         bar()
///     except NotImplementedError:
///         raise
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     bar()
/// ```
#[violation]
pub struct UselessTryExcept;

impl Violation for UselessTryExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove exception handler; error is immediately re-raised")
    }
}

/// TRY302
pub(crate) fn useless_try_except(checker: &mut Checker, handlers: &[Excepthandler]) {
    let handler_errs = handlers
        .iter()
        .map(
            |ExceptHandler(ExcepthandlerExceptHandler { name, body, .. })| {
                let stmt = &body.first()?;
                let Stmt::Raise(ast::StmtRaise {  exc, .. }) = &stmt else {
                    return None;
                };

                if let Some(expr) = exc {
                    // E.g., `except ... as e: raise e`
                    if let Expr::Name(ast::ExprName { id, .. }) = expr.as_ref() {
                        if Some(id) == name.as_ref() {
                            return Some(Diagnostic::new(UselessTryExcept, stmt.range()));
                        }
                    }
                    None
                } else {
                    // E.g., `except ...: raise`
                    Some(Diagnostic::new(UselessTryExcept, stmt.range()))
                }
            },
        )
        .collect::<Vec<_>>();

    if handler_errs.iter().all(Option::is_some) {
        // All handlers have diagnostics
        for err in handler_errs {
            checker.diagnostics.push(err.unwrap());
        }
    }
}
