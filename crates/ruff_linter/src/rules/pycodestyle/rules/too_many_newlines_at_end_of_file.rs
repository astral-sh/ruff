use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// ## What it does
/// Checks for files with multiple trailing blank lines.
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
#[violation]
pub struct TooManyNewlinesAtEndOfFile {
    num_trailing_newlines: u32,
}

impl AlwaysFixableViolation for TooManyNewlinesAtEndOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyNewlinesAtEndOfFile {
            num_trailing_newlines,
        } = self;

        // We expect a single trailing newline; so two trailing newlines is one too many, three
        // trailing newlines is two too many, etc.
        if *num_trailing_newlines > 2 {
            format!("Too many newlines at end of file")
        } else {
            format!("Extra newline at end of file")
        }
    }

    fn fix_title(&self) -> String {
        let TooManyNewlinesAtEndOfFile {
            num_trailing_newlines,
        } = self;
        if *num_trailing_newlines > 2 {
            "Remove trailing newlines".to_string()
        } else {
            "Remove trailing newline".to_string()
        }
    }
}

/// W391
pub(crate) fn too_many_newlines_at_end_of_file(diagnostics: &mut Vec<Diagnostic>, tokens: &Tokens) {
    let mut num_trailing_newlines = 0u32;
    let mut start: Option<TextSize> = None;
    let mut end: Option<TextSize> = None;

    // Count the number of trailing newlines.
    for token in tokens.iter().rev() {
        match token.kind() {
            TokenKind::NonLogicalNewline | TokenKind::Newline => {
                if num_trailing_newlines == 0 {
                    end = Some(token.end());
                }
                start = Some(token.end());
                num_trailing_newlines += 1;
            }
            TokenKind::Dedent => continue,
            _ => {
                break;
            }
        }
    }

    if num_trailing_newlines == 0 || num_trailing_newlines == 1 {
        return;
    }

    let range = match (start, end) {
        (Some(start), Some(end)) => TextRange::new(start, end),
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        TooManyNewlinesAtEndOfFile {
            num_trailing_newlines,
        },
        range,
    );
    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(range)));
    diagnostics.push(diagnostic);
}
