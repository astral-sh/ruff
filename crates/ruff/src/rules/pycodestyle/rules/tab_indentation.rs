use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Indexer;
use ruff_python_whitespace::{leading_indentation, Line};

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
