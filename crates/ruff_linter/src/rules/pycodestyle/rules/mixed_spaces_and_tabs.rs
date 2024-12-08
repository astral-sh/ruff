use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::leading_indentation;
use ruff_source_file::Line;

/// ## What it does
/// Checks for mixed tabs and spaces in indentation.
///
/// ## Why is this bad?
/// Never mix tabs and spaces.
///
/// The most popular way of indenting Python is with spaces only. The
/// second-most popular way is with tabs only. Code indented with a
/// mixture of tabs and spaces should be converted to using spaces
/// exclusively.
///
/// ## Example
/// ```python
/// if a == 0:\n        a = 1\n\tb = 1
/// ```
///
/// Use instead:
/// ```python
/// if a == 0:\n    a = 1\n    b = 1
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MixedSpacesAndTabs;

impl Violation for MixedSpacesAndTabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Indentation contains mixed spaces and tabs".to_string()
    }
}

/// E101
pub(crate) fn mixed_spaces_and_tabs(line: &Line) -> Option<Diagnostic> {
    let indent = leading_indentation(line.as_str());

    if indent.contains(' ') && indent.contains('\t') {
        Some(Diagnostic::new(
            MixedSpacesAndTabs,
            TextRange::at(line.start(), indent.text_len()),
        ))
    } else {
        None
    }
}
