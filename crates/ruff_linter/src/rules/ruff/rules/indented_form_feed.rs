use memchr::memchr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_source_file::Line;
use ruff_text_size::{TextRange, TextSize};

use crate::{Violation, checkers::ast::LintContext};

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
#[violation_metadata(preview_since = "0.9.6")]
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
pub(crate) fn indented_form_feed(line: &Line, context: &LintContext) {
    let Some(index_relative_to_line) = memchr(FORM_FEED, line.as_bytes()) else {
        return;
    };

    if index_relative_to_line == 0 {
        return;
    }

    if line[..index_relative_to_line]
        .as_bytes()
        .iter()
        .any(|byte| *byte != SPACE && *byte != TAB)
    {
        return;
    }

    let Ok(relative_index) = u32::try_from(index_relative_to_line) else {
        return;
    };
    let absolute_index = line.start() + TextSize::new(relative_index);
    let range = TextRange::at(absolute_index, 1.into());

    context.report_diagnostic(IndentedFormFeed, range);
}
