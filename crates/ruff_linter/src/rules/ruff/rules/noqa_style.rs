use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::noqa::Directive;

/// ## What it does
/// Checks for `noqa` directives with multiple spaces between the colon and the codes.
///
/// ## Why is this bad?
/// Multiple spaces between the colon and the codes are redundant and lead to longer lines.
///
/// ## Example
/// ```python
/// x = 2  # noqa:  X600
/// ```
///
/// Use instead:
/// x = 2  # noqa: X600
/// ```
///
#[violation]
pub struct MultipleSpacesBeforeNOQACode;

impl AlwaysFixableViolation for MultipleSpacesBeforeNOQACode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` directives should only have one space before the code(s)")
    }

    fn fix_title(&self) -> String {
        "Remove extra space(s) before code(s)".to_string()
    }
}

/// ## What it does
/// Checks for `noqa` directives no space between the colon and the codes.
///
/// ## Why is this bad?
/// Missing space between the colon and the code(s) reduces makes the directive harder to read.
///
/// ## Example
/// ```python
/// x = 2  # noqa:X600
/// ```
///
/// Use instead:
/// x = 2  # noqa: X600
/// ```
///
#[violation]
pub struct MissingSpaceBeforeNOQACode;

impl AlwaysFixableViolation for MissingSpaceBeforeNOQACode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` directives should have one space before the code(s)")
    }

    fn fix_title(&self) -> String {
        "Add missing space before code(s)".to_string()
    }
}

/// RUF029, RUF030
pub(crate) fn noqa_style(diagnostics: &mut Vec<Diagnostic>, indexer: &Indexer, locator: &Locator) {
    for range in indexer.comment_ranges() {
        let line = locator.slice(*range);
        let offset = range.start();
        if let Ok(Some(Directive::Codes(codes))) = Directive::try_extract(line, TextSize::new(0)) {
            let mut cursor = codes.range().start().to_usize();
            cursor += find_noqa_end(line).unwrap();
            let num_spaces = leading_whitespace_len(&line[cursor..]);

            // RUF030
            if num_spaces == 0 {
                let start = offset + TextSize::new(u32::try_from(cursor - 1).unwrap());
                let end = start + TextSize::new(1);
                let mut diagnostic =
                    Diagnostic::new(MissingSpaceBeforeNOQACode, TextRange::new(start, end));
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(' '.to_string(), end)));
                diagnostics.push(diagnostic);
            }
            // RUF029
            else if num_spaces > 1 {
                let start = offset + TextSize::new(u32::try_from(cursor + 1).unwrap());
                let end = start + TextSize::new(u32::try_from(num_spaces - 1).unwrap());
                let mut diagnostic =
                    Diagnostic::new(MultipleSpacesBeforeNOQACode, TextRange::new(start, end));
                diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
                diagnostics.push(diagnostic);
            }
        }
    }
}

fn leading_whitespace_len(text: &str) -> usize {
    text.find(|c: char| !c.is_whitespace()).unwrap_or(0)
}

fn find_noqa_end(text: &str) -> Option<usize> {
    for (char_index, char) in text.char_indices() {
        // Only bother checking for the `noqa` literal if the character is `n` or `N`.
        if !matches!(char, 'n' | 'N') {
            continue;
        }

        // Determine the start of the `noqa` literal.
        if !matches!(
            text[char_index..].as_bytes(),
            [b'n' | b'N', b'o' | b'O', b'q' | b'Q', b'a' | b'A', b':', ..]
        ) {
            continue;
        }

        return Some(char_index + "noqa:".len());
    }
    None
}
