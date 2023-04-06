use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::whitespace::leading_space;

#[violation]
pub struct TabIndentation;

impl Violation for TabIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains tabs")
    }
}

/// W191
pub fn tab_indentation(lineno: usize, line: &str, string_ranges: &[Range]) -> Option<Diagnostic> {
    let indent = leading_space(line);
    if let Some(tab_index) = indent.find('\t') {
        // If the tab character is within a multi-line string, abort.
        if let Ok(range_index) = string_ranges.binary_search_by(|range| {
            let start = range.location.row();
            let end = range.end_location.row();
            if start > lineno {
                std::cmp::Ordering::Greater
            } else if end < lineno {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            let string_range = &string_ranges[range_index];
            let start = string_range.location;
            let end = string_range.end_location;

            // Tab is contained in the string range by virtue of lines.
            if lineno != start.row() && lineno != end.row() {
                return None;
            }

            let tab_column = line[..tab_index].chars().count();

            // Tab on first line of the quoted range, following the quote.
            if lineno == start.row() && tab_column > start.column() {
                return None;
            }

            // Tab on last line of the quoted range, preceding the quote.
            if lineno == end.row() && tab_column < end.column() {
                return None;
            }
        }

        Some(Diagnostic::new(
            TabIndentation,
            Range::new(
                Location::new(lineno, 0),
                Location::new(lineno, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}
