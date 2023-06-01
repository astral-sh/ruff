use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct LongNumericLiteralsInStub;

/// ## What it does
/// Checks for numeric literals longer than 10 characters
///
/// ## Why is this bad?
/// If a function has a default value where the string representation is greater than 10
/// characters, it is likely to be an implementation detail or a constant that varies depending on
/// the system you're running on, such as `sys.maxsize`. Consider replacing them with ellipses.
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
impl Violation for LongNumericLiteralsInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Numeric literals with a string representation longer than ten characters are not permitted")
    }
}

/// PYI054
pub(crate) fn long_numeric_literals_in_stub(checker: &mut Checker, range: TextRange) {
    if range.len() > TextSize::new(10) {
        checker
            .diagnostics
            .push(Diagnostic::new(LongNumericLiteralsInStub, range));
    }
}
