use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::{UniversalNewlineIterator, UniversalNewlines};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for docstring summary lines that are not separated from the docstring
/// description by one blank line.
///
/// ## Why is this bad?
/// [PEP 257] recommends that multi-line docstrings consist of "a summary line
/// just like a one-line docstring, followed by a blank line, followed by a
/// more elaborate description."
///
/// ## Example
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///     Sort the list in ascending order and return a copy of the
///     result using the bubble sort algorithm.
///     """
/// ```
///
/// Use instead:
/// ```python
/// def sort_list(l: list[int]) -> list[int]:
///     """Return a sorted copy of the list.
///
///     Sort the list in ascending order and return a copy of the
///     result using the bubble sort algorithm.
///     """
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct BlankLineAfterSummary {
    num_lines: usize,
}

impl Violation for BlankLineAfterSummary {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSummary { num_lines } = self;
        if *num_lines == 0 {
            format!("1 blank line required between summary line and description")
        } else {
            format!(
                "1 blank line required between summary line and description (found {num_lines})"
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Insert single blank line".to_string())
    }
}

/// D205
pub(crate) fn blank_after_summary(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    if !docstring.triple_quoted() {
        return;
    }

    let mut lines_count: usize = 1;
    let mut blanks_count = 0;
    for line in body.trim().universal_newlines().skip(1) {
        lines_count += 1;
        if line.trim().is_empty() {
            blanks_count += 1;
        } else {
            break;
        }
    }
    if lines_count > 1 && blanks_count != 1 {
        let mut diagnostic = Diagnostic::new(
            BlankLineAfterSummary {
                num_lines: blanks_count,
            },
            docstring.range(),
        );
        if blanks_count > 1 {
            let mut lines = UniversalNewlineIterator::with_offset(&body, body.start());
            let mut summary_end = body.start();

            // Find the "summary" line (defined as the first non-blank line).
            for line in lines.by_ref() {
                if !line.trim().is_empty() {
                    summary_end = line.full_end();
                    break;
                }
            }

            // Find the last blank line
            let mut blank_end = summary_end;
            for line in lines {
                if !line.trim().is_empty() {
                    blank_end = line.start();
                    break;
                }
            }

            // Insert one blank line after the summary (replacing any existing lines).
            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                checker.stylist().line_ending().to_string(),
                summary_end,
                blank_end,
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
