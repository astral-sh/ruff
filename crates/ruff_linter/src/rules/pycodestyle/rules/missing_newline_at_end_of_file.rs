use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_codegen::Stylist;
use ruff_text_size::{TextLen, TextRange};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for files missing a new line at the end of the file.
///
/// ## Why is this bad?
/// Trailing blank lines in a file are superfluous.
///
/// However, the last line of the file should end with a newline.
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
#[derive(ViolationMetadata)]
pub(crate) struct MissingNewlineAtEndOfFile;

impl AlwaysFixableViolation for MissingNewlineAtEndOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        "No newline at end of file".to_string()
    }

    fn fix_title(&self) -> String {
        "Add trailing newline".to_string()
    }
}

/// W292
pub(crate) fn no_newline_at_end_of_file(
    locator: &Locator,
    stylist: &Stylist,
    context: &LintContext,
) {
    let source = locator.contents();

    // Ignore empty and BOM only files.
    if source.is_empty() || source == "\u{feff}" {
        return;
    }

    if !source.ends_with(['\n', '\r']) {
        let range = TextRange::empty(locator.contents().text_len());

        let mut diagnostic = context.report_diagnostic(MissingNewlineAtEndOfFile, range);
        diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
            stylist.line_ending().to_string(),
            range.start(),
        )));
    }
}
