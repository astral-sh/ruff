use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct InvalidAllFormat;

impl Violation for InvalidAllFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid format for `__all__`, must be `tuple` or `list`")
    }
}

/// PLE0605
pub fn invalid_all_format(expr: &Expr) -> Diagnostic {
    Diagnostic::new(InvalidAllFormat, expr.range())
}
