use ruff_diagnostics::{Diagnostic, Violation};
use ruff_text_size::{TextRange, TextSize};

use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct LongNumericLiteralsInStub;

/// ## What it does
/// Checks for numeric literals longer than 10 characters
///
/// ## Why is this bad?
/// Long hardcoded numeric values are unlikely to be useful for users. Consider replacing them with ellipses.
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
