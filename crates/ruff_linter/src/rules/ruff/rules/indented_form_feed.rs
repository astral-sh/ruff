use memchr::memchr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_source_file::Line;
use ruff_text_size::{TextRange, TextSize};

/// ## What it does
/// Checks for form feed characters preceded by either a space or a tab.
///
/// ## Why is this bad?
/// [The language reference][lexical-analysis-indentation] states:
///
/// > A formfeed character may be present at the start of the line;
/// > it will be ignored for the indentation calculations above.
/// > Formfeed characters occurring elsewhere in the leading whitespace
/// > have an undefined effect (for instance, they may reset the space count to zero).
///
/// ## Example
///
/// ```python
/// if foo():\n    \fbar()
/// ```
///
/// Use instead:
///
/// ```python
/// if foo():\n    bar()
/// ```
///
/// [lexical-analysis-indentation]: https://docs.python.org/3/reference/lexical_analysis.html#indentation
#[derive(ViolationMetadata)]
pub(crate) struct IndentedFormFeed;

impl Violation for IndentedFormFeed {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Indented form feed".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove form feed".to_string())
    }
}

const FORM_FEED: u8 = b'\x0c';
const SPACE: u8 = b' ';
const TAB: u8 = b'\t';

/// RUF054
pub(crate) fn indented_form_feed(line: &Line) -> Option<Diagnostic> {
    let index_relative_to_line = memchr(FORM_FEED, line.as_bytes())?;

    if index_relative_to_line == 0 {
        return None;
    }

    if line[..index_relative_to_line]
        .as_bytes()
        .iter()
        .any(|byte| *byte != SPACE && *byte != TAB)
    {
        return None;
    }

    let relative_index = u32::try_from(index_relative_to_line).ok()?;
    let absolute_index = line.start() + TextSize::new(relative_index);
    let range = TextRange::at(absolute_index, 1.into());

    Some(Diagnostic::new(IndentedFormFeed, range))
}
