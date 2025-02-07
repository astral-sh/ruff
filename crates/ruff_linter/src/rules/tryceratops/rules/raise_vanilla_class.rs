use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for code that raises `Exception` or `BaseException` directly.
///
/// ## Why is this bad?
/// Handling such exceptions requires the use of `except Exception` or
/// `except BaseException`. These will capture almost _any_ raised exception,
/// including failed assertions, division by zero, and more.
///
/// Prefer to raise your own exception, or a more specific built-in
/// exception, so that you can avoid over-capturing exceptions that you
/// don't intend to handle.
///
/// ## Example
/// ```python
/// def main_function():
///     if not cond:
///         raise Exception()
///
///
/// def consumer_func():
///     try:
///         do_step()
///         prepare()
///         main_function()
///     except Exception:
///         logger.error("Oops")
/// ```
///
/// Use instead:
/// ```python
/// def main_function():
///     if not cond:
///         raise CustomException()
///
///
/// def consumer_func():
///     try:
///         do_step()
///         prepare()
///         main_function()
///     except CustomException:
///         logger.error("Main function failed")
///     except Exception:
///         logger.error("Oops")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct RaiseVanillaClass;

impl Violation for RaiseVanillaClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Create your own exception".to_string()
    }
}

/// TRY002
pub(crate) fn raise_vanilla_class(checker: &Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_qualified_name(map_callable(expr))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins", "Exception" | "BaseException"]
            )
        })
    {
        checker.report_diagnostic(Diagnostic::new(RaiseVanillaClass, expr.range()));
    }
}
