use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::Line;

/// ## What it does
/// Checks for indentation that uses tabs.
///
/// ## Why is this bad?
/// According to [PEP 8], spaces are preferred over tabs (unless used to remain
/// consistent with code that is already indented with tabs).
///
/// ## Example
/// ```python
/// if True:
/// 	a = 1
/// ```
///
/// Use instead:
/// ```python
/// if True:
///     a = 1
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#tabs-or-spaces
#[violation]
pub struct TabIndentation;

impl Violation for TabIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains tabs")
    }
}

/// W191
pub(crate) fn tab_indentation(line: &Line, indexer: &Indexer) -> Option<Diagnostic> {
    let indent = leading_indentation(line);
    if let Some(tab_index) = indent.find('\t') {
        // If the tab character is within a multi-line string, abort.
        let tab_offset = line.start() + TextSize::try_from(tab_index).unwrap();
        if indexer.triple_quoted_string_range(tab_offset).is_none() {
            return Some(Diagnostic::new(
                TabIndentation,
                TextRange::at(line.start(), indent.text_len()),
            ));
        }
    }
    None
}
