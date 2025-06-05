use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::except;
use ruff_python_ast::{self as ast, ExceptHandler};

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` blocks that handle all exceptions, but are not the last
/// `except` block in a `try` statement.
///
/// ## Why is this bad?
/// When an exception is raised within a `try` block, the `except` blocks are
/// evaluated in order, and the first matching block is executed. If an `except`
/// block handles all exceptions, but isn't the last block, Python will raise a
/// `SyntaxError`, as the following blocks would never be executed.
///
/// ## Example
/// ```python
/// def reciprocal(n):
///     try:
///         reciprocal = 1 / n
///     except:
///         print("An exception occurred.")
///     except ZeroDivisionError:
///         print("Cannot divide by zero.")
///     else:
///         return reciprocal
/// ```
///
/// Use instead:
/// ```python
/// def reciprocal(n):
///     try:
///         reciprocal = 1 / n
///     except ZeroDivisionError:
///         print("Cannot divide by zero.")
///     except:
///         print("An exception occurred.")
///     else:
///         return reciprocal
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[derive(ViolationMetadata)]
pub(crate) struct DefaultExceptNotLast;

impl Violation for DefaultExceptNotLast {
    #[derive_message_formats]
    fn message(&self) -> String {
        "An `except` block as not the last exception handler".to_string()
    }
}

/// F707
pub(crate) fn default_except_not_last(
    checker: &Checker,
    handlers: &[ExceptHandler],
    locator: &Locator,
) {
    for (idx, handler) in handlers.iter().enumerate() {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) = handler;
        if type_.is_none() && idx < handlers.len() - 1 {
            checker.report_diagnostic(DefaultExceptNotLast, except(handler, locator.contents()));
        }
    }
}
