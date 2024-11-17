use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Example
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct BadNumericLiteralFormat;

impl Violation for BadNumericLiteralFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("TODO: write message: {}", todo!("implement message"))
    }
}

/// WPS987
pub(crate) fn bad_numeric_literal_format(checker: &mut Checker) {}
