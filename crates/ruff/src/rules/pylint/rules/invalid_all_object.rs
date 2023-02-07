use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
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
pub fn invalid_all_object(checker: &mut Checker, expr: &Expr) {
    checker
        .diagnostics
        .push(Diagnostic::new(InvalidAllObject, Range::from_located(expr)));
}
