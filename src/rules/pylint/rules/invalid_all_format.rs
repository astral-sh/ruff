use ruff_macros::derive_message_formats;
use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
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
pub fn invalid_all_format(checker: &mut Checker, expr: &Expr) {
    checker
        .diagnostics
        .push(Diagnostic::new(InvalidAllFormat, Range::from_located(expr)));
}
