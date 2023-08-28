use ruff_python_ast::Expr;
use ruff_text_size::{Ranged, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NumericLiteralTooLong;

/// ## What it does
/// Checks for numeric literals with a string representation longer than ten
/// characters.
///
/// ## Why is this bad?
/// If a function has a default value where the literal representation is
/// greater than 50 characters, it is likely to be an implementation detail or
/// a constant that varies depending on the system you're running on.
///
/// Consider replacing such constants with ellipses (`...`).
///
/// ## Example
/// ```python
/// def foo(arg: int = 12345678901) -> None: ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(arg: int = ...) -> None: ...
/// ```
impl AlwaysAutofixableViolation for NumericLiteralTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Numeric literals with a string representation longer than ten characters are not permitted")
    }

    fn autofix_title(&self) -> String {
        "Replace with `...`".to_string()
    }
}

/// PYI054
pub(crate) fn numeric_literal_too_long(checker: &mut Checker, expr: &Expr) {
    if expr.range().len() <= TextSize::new(10) {
        return;
    }

    let mut diagnostic = Diagnostic::new(NumericLiteralTooLong, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            "...".to_string(),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
