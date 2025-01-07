use std::iter::Peekable;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_notebook::CellOffsets;
use ruff_python_parser::{Token, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// ## What it does
/// Checks for files with multiple trailing blank lines.
///
/// In the case of notebooks, this check is applied to
/// each cell separately.
///
/// ## Why is this bad?
/// Trailing blank lines in a file are superfluous.
///
/// However, the last line of the file should end with a newline.
///
/// ## Example
/// ```python
/// spam(1)\n\n\n
/// ```
///
/// Use instead:
/// ```python
/// spam(1)\n
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct TooManyNewlinesAtEndOfFile {
    num_trailing_newlines: u32,
    in_notebook: bool,
}

impl AlwaysFixableViolation for TooManyNewlinesAtEndOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let domain = if self.in_notebook { "cell" } else { "file" };
        // We expect a single trailing newline; so two trailing newlines is one too many, three
        // trailing newlines is two too many, etc.
        if self.num_trailing_newlines > 2 {
            format!("Too many newlines at end of {domain}")
        } else {
            format!("Extra newline at end of {domain}")
        }
    }

    fn fix_title(&self) -> String {
        let title = if self.num_trailing_newlines > 2 {
            "Remove trailing newlines"
        } else {
            "Remove trailing newline"
        };
        title.to_string()
    }
}

/// W391
pub(crate) fn too_many_newlines_at_end_of_file(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &Tokens,
    cell_offsets: Option<&CellOffsets>,
) {
    let Some(last_token) = tokens.last() else {
        return;
    };
    let last_textsize = last_token.end();

    let tokens_iter = tokens.iter().rev().peekable();

    let newline_diagnostics = if let Some(cell_offsets) = cell_offsets {
        let offset_iter = cell_offsets.iter().rev().peekable();
        collect_trailing_newlines_diagnostics(tokens_iter, offset_iter)
    } else {
        // To handle notebooks and ordinary source files uniformly,
        // we "convert" the source type to a notebook by appending
        // a newline.
        let last_textsize = last_textsize + TextSize::from(1);
        let offset_iter = std::iter::once(&last_textsize).peekable();
        collect_trailing_newlines_diagnostics(tokens_iter, offset_iter)
    };
    diagnostics.extend(newline_diagnostics);
}

fn collect_trailing_newlines_diagnostics<'a>(
    mut tokens_iter: Peekable<impl Iterator<Item = &'a Token>>,
    offset_iter: Peekable<impl Iterator<Item = &'a TextSize>>,
) -> Vec<Diagnostic> {
    let mut results = Vec::new();

    // NB: When interpreting the below, recall that the iterators
    // passed into this function are reversed.
    for &offset in offset_iter {
        // Advance to the offset
        while let Some(next_token) = tokens_iter.peek() {
            if next_token.end() >= offset {
                tokens_iter.next();
            } else {
                break;
            }
        }

        let mut num_trailing_newlines: u32 = 0;
        let mut newline_range_start: Option<TextSize> = None;
        let mut newline_range_end: Option<TextSize> = None;

        while let Some(next_token) = tokens_iter.peek() {
            match next_token.kind() {
                TokenKind::Newline | TokenKind::NonLogicalNewline => {
                    if newline_range_end.is_none() {
                        newline_range_end = Some(next_token.end());
                    }
                    newline_range_start = Some(next_token.end());

                    tokens_iter.next();
                    num_trailing_newlines += 1;
                }
                TokenKind::Dedent => {
                    tokens_iter.next();
                }
                _ => {
                    break;
                }
            }
        }

        if num_trailing_newlines == 0 || num_trailing_newlines == 1 {
            continue;
        };

        let Some((start, end)) = (match (newline_range_start, newline_range_end) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        }) else {
            continue;
        };

        let diagnostic_range = TextRange::new(start, end);

        results.push(
            Diagnostic::new(
                TooManyNewlinesAtEndOfFile {
                    num_trailing_newlines,
                },
                diagnostic_range,
            )
            .with_fix(Fix::safe_edit(Edit::range_deletion(diagnostic_range))),
        );
    }
    results
}
