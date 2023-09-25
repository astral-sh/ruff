use super::{LogicalLine, LogicalLineToken};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for continuation lines not indented as far as they should be or indented too far.
///
/// ## Why is this bad?
/// This makes reading code harder.
///
/// ## Example
/// ```python
/// print("Python", (
/// "Rules"))
/// ```
///
/// Use instead:
/// ```python
/// print("Python", (
///     "Rules"))
/// ```
#[violation]
pub struct MissingOrOutdentedIndentation;

impl Violation for MissingOrOutdentedIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continuation line missing indentation or outdented.")
    }
}

/// E122
pub(crate) fn continuation_line_missing_indentation_or_outdented(
    context: &mut LogicalLinesContext,
    logical_line: &LogicalLine,
    indent_char: char,
    indent_size: usize,
) {
}
