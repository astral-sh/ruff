use rustpython_parser::ast::{self, Excepthandler};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::except_range;
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for bare `except` blocks that are not the last exception handler.
///
/// ## Why is this bad?
/// `except` blocks are executed in order, so if a `except` block that handles
/// all exceptions is not the last one, it will prevent the following `except`
/// blocks from being executed. This will result in a `SyntaxError`.
///
/// ## Example
/// ```python
/// def reciprocal(n):
///     try:
///         reciprocal = 1 / n
///     except:  # SyntaxError
///         print("An exception occurred.")
///     except ZeroDivisionError:
///         print("Cannot divide by zero!")
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
///         print("Cannot divide by zero!")
///     except:
///         print("An exception occurred.")
///     else:
///         return reciprocal
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[violation]
pub struct DefaultExceptNotLast;

impl Violation for DefaultExceptNotLast {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("An `except` block as not the last exception handler")
    }
}

/// F707
pub(crate) fn default_except_not_last(
    handlers: &[Excepthandler],
    locator: &Locator,
) -> Option<Diagnostic> {
    for (idx, handler) in handlers.iter().enumerate() {
        let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = handler;
        if type_.is_none() && idx < handlers.len() - 1 {
            return Some(Diagnostic::new(
                DefaultExceptNotLast,
                except_range(handler, locator),
            ));
        }
    }

    None
}
