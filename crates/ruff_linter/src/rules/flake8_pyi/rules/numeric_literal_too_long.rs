use ruff_python_ast::Expr;
use ruff_text_size::{Ranged, TextSize};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for numeric literals with a string representation longer than ten
/// characters.
///
/// ## Why is this bad?
/// If a function has a default value where the literal representation is
/// greater than 10 characters, the value is likely to be an implementation
/// detail or a constant that varies depending on the system you're running on.
///
/// Default values like these should generally be omitted from stubs. Use
/// ellipses (`...`) instead.
///
/// ## Example
///
/// ```pyi
/// def foo(arg: int = 693568516352839939918568862861217771399698285293568) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def foo(arg: int = ...) -> None: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271")]
pub(crate) struct NumericLiteralTooLong;

impl AlwaysFixableViolation for NumericLiteralTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Numeric literals with a string representation longer than ten characters are not permitted"
            .to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `...`".to_string()
    }
}

/// PYI054
pub(crate) fn numeric_literal_too_long(checker: &Checker, expr: &Expr) {
    if expr.range().len() <= TextSize::new(10) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(NumericLiteralTooLong, expr.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "...".to_string(),
        expr.range(),
    )));
}
