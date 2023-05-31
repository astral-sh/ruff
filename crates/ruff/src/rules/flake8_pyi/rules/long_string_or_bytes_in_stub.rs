use ruff_diagnostics::{Diagnostic, Violation};
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct LongStringOrBytesInStub;

/// ## What it does
/// Checks for `str` or `bytes` literals longer than 50 characters in stubs.
///
/// ## Why is this bad?
/// Extremely long strings used as argument defaults or otherwise are unlikely to be useful for
/// users. Consider replacing them with ellipses.
impl Violation for LongStringOrBytesInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`str` and `bytes` literals longer than 50 characters should not be used in stubs.")
    }
}

/// PYI053
pub(crate) fn long_string_or_bytes_in_stub(checker: &mut Checker, expr: &Expr) {
    let length = match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(s),
            ..
        }) => s.chars().count(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bytes(bytes),
            ..
        }) => bytes.len(),
        _ => return,
    };

    if length > 50 {
        checker
            .diagnostics
            .push(Diagnostic::new(LongStringOrBytesInStub, expr.range()));
    }
}
