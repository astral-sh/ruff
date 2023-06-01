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
/// If a function has a default value where the string or bytes representation is greater than 50
/// characters, it is likely to be an implementation detail or a constant that varies depending on
/// the system you're running on. Consider replacing them with ellipses.
///
/// ## Example
/// ```python
/// def foo(arg: str = "51 character stringgggggggggggggggggggggggggggggggg") -> None: ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(arg: str = ...) -> None: ...
/// ```
impl Violation for LongStringOrBytesInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`str` and `bytes` literals longer than 50 characters should not be used in stubs")
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
