use rustpython_parser::ast::Expr;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct InvalidAllFormat;
);
impl Violation for InvalidAllFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid format for `__all__`, must be `tuple` or `list`")
    }
}

/// PLE0605
pub fn invalid_all_format(expr: &Expr) -> Diagnostic {
    Diagnostic::new(InvalidAllFormat, Range::from_located(expr))
}
