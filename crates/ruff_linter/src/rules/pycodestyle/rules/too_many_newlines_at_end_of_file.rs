use std::iter::Peekable;

use itertools::Itertools;
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
    let mut tokens_iter = tokens.iter().rev().peekable();

    if let Some(cell_offsets) = cell_offsets {
        diagnostics.extend(notebook_newline_diagnostics(tokens_iter, cell_offsets));
    } else if let Some(diagnostic) = newline_diagnostic(&mut tokens_iter, false) {
        diagnostics.push(diagnostic);
    }
}

/// Collects trailing newline diagnostics for each cell
fn notebook_newline_diagnostics<'a>(
    mut tokens_iter: Peekable<impl Iterator<Item = &'a Token>>,
    cell_offsets: &CellOffsets,
) -> Vec<Diagnostic> {
    let mut results = Vec::new();
    let offset_iter = cell_offsets.iter().rev();

    // NB: When interpreting the below, recall that the iterators
    // have been reversed.
    for &offset in offset_iter {
        // Advance to offset
        tokens_iter
            .peeking_take_while(|tok| tok.end() >= offset)
            .for_each(drop);

        let Some(diagnostic) = newline_diagnostic(&mut tokens_iter, true) else {
            continue;
        };

        results.push(diagnostic);
    }
    results
}

/// Possible diagnostic, with fix, for too many newlines in cell or source file
fn newline_diagnostic<'a>(
    tokens_iter: &mut Peekable<impl Iterator<Item = &'a Token>>,
    in_notebook: bool,
) -> Option<Diagnostic> {
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
        return None;
    }

    let (start, end) = (match (newline_range_start, newline_range_end) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => None,
    })?;

    let diagnostic_range = TextRange::new(start, end);
    Some(
        Diagnostic::new(
            TooManyNewlinesAtEndOfFile {
                num_trailing_newlines,
                in_notebook,
            },
            diagnostic_range,
        )
        .with_fix(Fix::safe_edit(Edit::range_deletion(diagnostic_range))),
    )
}
