use memchr::memchr;
use memchr::memchr_iter;

use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::{Ranged, TextRange, TextSize};

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
const NEW_LINE: u8 = b'\n';

/// RUF054
pub(crate) fn indented_form_feed(line: &LogicalLine) -> Option<Diagnostic> {
    println!("{:?}", line.tokens());
    println!("{:?}", line.text());
    println!("-----------");
    let bytes_line = line.text().as_bytes();
    let end_physical_line = memchr(NEW_LINE, bytes_line)?;
    let form_feed_indexes: Vec<_>  = memchr_iter(FORM_FEED, &bytes_line[..end_physical_line]).collect();

    if form_feed_indexes.len() == 0 || form_feed_indexes.len() == 1 && form_feed_indexes[0] == 0 {
        return None;
    }

    for j in &form_feed_indexes[1..] {
        if bytes_line[j-1] == TAB || bytes_line[j - 1] == SPACE {
            let relative_index = u32::try_from(*j).ok()?;
            let absolute_index = TextSize::new(relative_index);
            let range = TextRange::at(
                line.first_token().unwrap().range().start() + absolute_index,
                1.into(),
            );
            return Some(Diagnostic::new(IndentedFormFeed, range));
        }
    }

    None
}
