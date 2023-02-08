use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ### What it does
    /// Checks for bare exceptions.
    ///
    /// ## Why is this bad?
    /// It's hard to capture generic exceptions making it hard for handling specific scenarios.
    ///
    ///## Example
    ///```py
    /// def main_function():
    ///     if not cond:
    ///         raise Exception()
    /// def consumer_func():
    ///     try:
    ///         do_step()
    ///         prepare()
    ///         main_function()
    ///     except Exception:
    ///         logger.error("I have no idea what went wrong!!")
    ///```
    ///## How it should be
    ///```py
    /// def main_function():
    ///     if not cond:
    ///         raise CustomException()
    /// def consumer_func():
    ///     try:
    ///         do_step()
    ///         prepare()
    ///         main_function()
    ///     except CustomException:
    ///         logger.error("Main function failed")
    ///     except Exception:
    ///         logger.error("I have no idea what went wrong!!")
    pub struct RaiseVanillaClass;
);
impl Violation for RaiseVanillaClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Create your own exception")
    }
}

/// TRY002
pub fn raise_vanilla_class(checker: &mut Checker, expr: &Expr) {
    if checker
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["", "Exception"])
    {
        checker.diagnostics.push(Diagnostic::new(
            RaiseVanillaClass,
            Range::from_located(expr),
        ));
    }
}
