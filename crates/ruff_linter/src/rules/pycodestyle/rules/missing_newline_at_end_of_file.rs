use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;

/// ## What it does
/// Checks for files missing a new line at the end of the file.
///
/// ## Why is this bad?
/// Trailing blank lines are superfluous.
/// However the last line should end with a new line.
///
/// ## Example
/// ```python
/// spam(1)
/// ```
///
/// Use instead:
/// ```python
/// spam(1)\n
/// ```
#[violation]
pub struct MissingNewlineAtEndOfFile;

impl AlwaysAutofixableViolation for MissingNewlineAtEndOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No newline at end of file")
    }

    fn autofix_title(&self) -> String {
        "Add trailing newline".to_string()
    }
}

/// W292
pub(crate) fn no_newline_at_end_of_file(
    locator: &Locator,
    stylist: &Stylist,
    autofix: bool,
) -> Option<Diagnostic> {
    let source = locator.contents();

    // Ignore empty and BOM only files
    if source.is_empty() || source == "\u{feff}" {
        return None;
    }

    if !source.ends_with(['\n', '\r']) {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        // Both locations are at the end of the file (and thus the same).
        let range = TextRange::empty(locator.contents().text_len());

        let mut diagnostic = Diagnostic::new(MissingNewlineAtEndOfFile, range);
        if autofix {
            diagnostic.set_fix(Fix::automatic(Edit::insertion(
                stylist.line_ending().to_string(),
                range.start(),
            )));
        }
        return Some(diagnostic);
    }
    None
}
