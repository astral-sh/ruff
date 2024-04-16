use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for code that raises `Exception` directly.
///
/// ## Why is this bad?
/// Handling such exceptions requires the use of `except Exception`, which
/// captures _any_ raised exception, including failed assertions,
/// division by zero, and more.
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
#[violation]
pub struct RaiseVanillaClass;

impl Violation for RaiseVanillaClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Create your own exception")
    }
}

/// TRY002
pub(crate) fn raise_vanilla_class(checker: &mut Checker, expr: &Expr) {
    let node = if let Expr::Call(ast::ExprCall { func, .. }) = expr {
        func
    } else {
        expr
    };
    if checker.semantic().match_builtin_expr(node, "Exception") {
        checker
            .diagnostics
            .push(Diagnostic::new(RaiseVanillaClass, expr.range()));
    }
}
