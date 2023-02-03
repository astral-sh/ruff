use ruff_macros::derive_message_formats;
use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_simple_violation!(
    InvalidAllObject,
    "Invalid object in `__all__`, must contain only strings"
);

/// PLE0604
pub fn invalid_all_object(checker: &mut Checker, expr: &Expr) {
    checker
        .diagnostics
        .push(Diagnostic::new(InvalidAllObject, Range::from_located(expr)));
}
