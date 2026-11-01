use ruff_python_ast::{self as ast, ExprKind, NodeList};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "0.7.0", category = Category::Complexity)]
pub(crate) struct UselessTryExcept;

impl Violation for UselessTryExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Remove exception handler; error is immediately re-raised".to_string()
    }
}

/// TRY203 (previously TRY302)
pub(crate) fn useless_try_except(
    checker: &Checker,
    handlers: NodeList<'_, ruff_python_ast::kind::ExceptHandler>,
) {
    if handlers.iter().all(|handler| {
        let ast::ExceptHandlerFields::ExceptHandler(ast::ExceptHandlerExceptHandlerFields {
            name,
            body,
            ..
        }) = handler.fields();
        let Some(ast::StmtFields::Raise(ast::StmtRaiseFields {
            exc, cause: None, ..
        })) = body.first().map(ruff_python_ast::Stmt::fields)
        else {
            return false;
        };
        if let Some(expr) = exc {
            // E.g., `except ... as e: raise e`
            if let ExprKind::Name(name_node) = expr.classify() {
                let id = name_node.id();
                if name.is_some_and(|name| name.as_str() == id) {
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
            checker.report_diagnostic(UselessTryExcept, handler.range());
        }
    }
}
