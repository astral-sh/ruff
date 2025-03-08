use ruff_python_ast::{self as ast, ExceptHandler, ExceptHandlerExceptHandler, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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

/// Checks if the given exception handler immediately re-raises.
fn is_immediate_reraise_handler(handler: &ExceptHandler) -> bool {
    let ExceptHandler::ExceptHandler(ExceptHandlerExceptHandler { name, body, .. }) = handler;
    let Some(Stmt::Raise(ast::StmtRaise {
        exc, cause: None, ..
    })) = &body.first()
    else {
        return false;
    };

    let Some(exc) = exc else {
        return true;
    };

    let Expr::Name(ast::ExprName { id, .. }) = exc.as_ref() else {
        return false;
    };

    name.as_ref().is_some_and(|name| name.as_str() == id)
}

/// TRY203 (previously TRY302)
pub(crate) fn useless_try_except(checker: &Checker, handlers: &[ExceptHandler]) {
    // Iterate over `handlers` in reverse order and stop at the first non-immediate re-raise handler.
    //
    // ```python
    // try:
    //     ...
    // except ValueError:      # not useless
    //     raise
    // except Exceptions as e: # not useless (stop here)
    //     print(e)
    // except TypeError as e:  # useless
    //     raise e
    // except ImportError:     # useless
    //     raise
    // ```
    checker.report_diagnostics(
        handlers
            .iter()
            .rev()
            .take_while(|handler| is_immediate_reraise_handler(handler))
            .map(|handler| Diagnostic::new(UselessTryExcept, handler.range())),
    );
}
