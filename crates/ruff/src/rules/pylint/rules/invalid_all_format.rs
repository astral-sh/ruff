use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for invalid formats for `__all__`.
///
/// ## Why is this bad?
/// `__all__` should be a `tuple` or `list`.
///
/// ## Example
/// ```python
/// __all__ = "Foo"
/// ```
///
/// Use instead:
/// ```python
/// __all__ = ("Foo",)
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
#[violation]
pub struct InvalidAllFormat;

impl Violation for InvalidAllFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid format for `__all__`, must be `tuple` or `list`")
    }
}

/// PLE0605
pub(crate) fn invalid_all_format(expr: &Expr) -> Diagnostic {
    Diagnostic::new(InvalidAllFormat, expr.range())
}
