use rustpython_parser::ast::Location;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::types::Range;

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
pub fn no_newline_at_end_of_file(
    locator: &Locator,
    stylist: &Stylist,
    autofix: bool,
) -> Option<Diagnostic> {
    if !locator.contents().ends_with(['\n', '\r']) {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        if let Some(line) = locator.contents().universal_newlines().last() {
            // Both locations are at the end of the file (and thus the same).
            let location = Location::new(locator.count_lines(), line.len());
            let mut diagnostic =
                Diagnostic::new(MissingNewlineAtEndOfFile, Range::new(location, location));
            if autofix {
                diagnostic.set_fix(Edit::insertion(stylist.line_ending().to_string(), location));
            }
            return Some(diagnostic);
        }
    }
    None
}
