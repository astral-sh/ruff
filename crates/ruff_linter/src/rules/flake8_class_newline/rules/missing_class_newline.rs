use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::Ranged;

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::{LogicalLine};

/// ## What it does
/// Checks for a newline after a class definition.
///
/// ## Why is this important?
/// Adhering to PEP 8 guidelines helps maintain consistent and readable code.
/// Having a newline after a class definition is recommended by PEP 8.
///
/// ## Example
/// ```python
/// class MyClass:
///     def method(self):
///         return 'example'
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///
///     def method(self):
///         return 'example'
/// ```
///
/// ## References
/// - [PEP 8 - Blank Lines](https://peps.python.org/pep-0008/#blank-lines)
#[violation]
pub struct ClassNewline;

impl Violation for ClassNewline {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class definition does not have a new line")
    }
}

/// CNL100
pub(crate) fn missing_class_newline(
    line: &LogicalLine,
    prev_line: Option<&LogicalLine>,
    context: &mut LogicalLinesContext
) {
    if let Some(prev_line) = prev_line {
        if let Some(token) = line.tokens_trimmed().first() {
            if matches!(token.kind(), TokenKind::Def | TokenKind::At) {
                if let Some(token) = prev_line.tokens_trimmed().first() {
                    if matches!(token.kind(), TokenKind::Class) {
                        let diagnostic = Diagnostic::new(
                            ClassNewline,
                            token.range(),
                        );
                        context.push_diagnostic(diagnostic);
                    }
                }
            }
        }
    }
}
