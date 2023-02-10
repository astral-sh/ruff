use rustpython_parser::ast::Expr;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct InvalidAllObject;
);
impl Violation for InvalidAllObject {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid object in `__all__`, must contain only strings")
    }
}

/// PLE0604
pub fn invalid_all_object(expr: &Expr) -> Diagnostic {
    Diagnostic::new(InvalidAllObject, Range::from_located(expr))
}
