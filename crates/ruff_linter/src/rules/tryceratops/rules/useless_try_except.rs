use ruff_python_ast::{self as ast, ExceptHandler, ExceptHandlerExceptHandler, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

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
#[derive(ViolationMetadata)]
pub(crate) struct UselessTryExcept;

impl Violation for UselessTryExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Remove exception handler; error is immediately re-raised".to_string()
    }
}

/// TRY203 (previously TRY302)
pub(crate) fn useless_try_except(checker: &Checker, handlers: &[ExceptHandler]) {
    if handlers.iter().all(|handler| {
        let ExceptHandler::ExceptHandler(ExceptHandlerExceptHandler { name, body, .. }) = handler;
        let Some(Stmt::Raise(ast::StmtRaise {
            exc, cause: None, ..
        })) = &body.first()
        else {
            return false;
        };
        if let Some(expr) = exc {
            // E.g., `except ... as e: raise e`
            if let Expr::Name(ast::ExprName { id, .. }) = expr.as_ref() {
                if name.as_ref().is_some_and(|name| name.as_str() == id) {
                    return true;
                }
            }
            false
        } else {
            // E.g., `except ...: raise`
            true
        }
    }) {
        // Require that all handlers are useless, but create one diagnostic per handler.
        for handler in handlers {
            checker.report_diagnostic(Diagnostic::new(UselessTryExcept, handler.range()));
        }
    }
}
