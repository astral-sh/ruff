use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_index::Indexer;
use ruff_source_file::LineRanges;
use ruff_text_size::{TextRange, TextSize};

use crate::Locator;

/// ## What it does
/// Checks for indentation that uses tabs.
///
/// ## Why is this bad?
/// According to [PEP 8], spaces are preferred over tabs (unless used to remain
/// consistent with code that is already indented with tabs).
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// The rule is also incompatible with the [formatter] when using
/// `format.indent-style="tab"`.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#tabs-or-spaces
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[derive(ViolationMetadata)]
pub(crate) struct TabIndentation;

impl Violation for TabIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Indentation contains tabs".to_string()
    }
}

/// W191
pub(crate) fn tab_indentation(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    indexer: &Indexer,
) {
    let contents = locator.contents().as_bytes();
    let mut offset = 0;
    while let Some(index) = memchr::memchr(b'\t', &contents[offset..]) {
        // If we find a tab in the file, grab the entire line.
        let range = locator.full_line_range(TextSize::try_from(offset + index).unwrap());

        // Determine whether the tab is part of the line's indentation.
        if let Some(indent) = tab_indentation_at_line_start(range.start(), locator, indexer) {
            diagnostics.push(Diagnostic::new(TabIndentation, indent));
        }

        // Advance to the next line.
        offset = range.end().to_usize();
    }
}

/// If a line includes tabs in its indentation, returns the range of the
/// indent.
fn tab_indentation_at_line_start(
    line_start: TextSize,
    locator: &Locator,
    indexer: &Indexer,
) -> Option<TextRange> {
    let mut contains_tab = false;
    for (i, char) in locator.after(line_start).as_bytes().iter().enumerate() {
        match char {
            // If we find a tab character, report it as a violation.
            b'\t' => {
                contains_tab = true;
            }
            // If we find a space, continue.
            b' ' | b'\x0C' => {}
            // If we find a non-whitespace character, stop.
            _ => {
                if contains_tab {
                    let range = TextRange::at(line_start, TextSize::try_from(i).unwrap());
                    if !indexer.multiline_ranges().contains_range(range) {
                        return Some(range);
                    }
                }
                break;
            }
        }
    }
    None
}
