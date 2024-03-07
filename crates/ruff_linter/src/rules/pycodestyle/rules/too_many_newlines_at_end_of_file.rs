use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};

/// ## What it does
/// Checks for files with too many new lines at the end of the file.
///
/// ## Why is this bad?
/// Trailing blank lines are superfluous.
/// However the last line should end with a new line.
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
pub struct TooManyNewlinesAtEndOfFile;

impl AlwaysFixableViolation for TooManyNewlinesAtEndOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many newlines at the end of file")
    }

    fn fix_title(&self) -> String {
        "Remove extraneous trailing newlines".to_string()
    }
}

/// W391
pub(crate) fn too_many_newlines_at_end_of_file(
    diagnostics: &mut Vec<Diagnostic>,
    lxr: &[LexResult],
    locator: &Locator,
) {
    let source = locator.contents();

    // Ignore empty and BOM only files
    if source.is_empty() || source == "\u{feff}" {
        return;
    }

    let mut count = 0;
    let mut start_pos: Option<TextSize> = None;
    let mut end_pos: Option<TextSize> = None;

    for &(ref tok, range) in lxr.iter().rev().flatten() {
        match tok {
            Tok::NonLogicalNewline | Tok::Newline => {
                if count == 0 {
                    end_pos = Some(range.end());
                }
                start_pos = Some(range.end());
                count += 1;
            }
            Tok::Dedent => continue,
            _ => {
                break;
            }
        }
    }

    if count > 1 {
        let start = start_pos.unwrap();
        let end = end_pos.unwrap();
        let range = TextRange::new(start, end);
        let mut diagnostic = Diagnostic::new(TooManyNewlinesAtEndOfFile, range);
        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
        diagnostics.push(diagnostic);
    }
}
