use regex::Regex;

use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;

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
pub(crate) fn too_many_newlines_at_end_of_file(locator: &Locator) -> Option<Diagnostic> {
    let source = locator.contents();

    // Ignore empty and BOM only files
    if source.is_empty() || source == "\u{feff}" {
        return None;
    }

    // Regex to match multiple newline characters at the end of the file
    let newline_regex = Regex::new(r"(\r\n){2,}$|\n{2,}$|\r{2,}$").unwrap();

    if let Some(mat) = newline_regex.find(source) {
        let start_pos = TextSize::new(mat.start() as u32 + 1);
        let end_pos = TextSize::new(mat.end() as u32);

        // Calculate the TextRange to keep only one newline at the end
        let range = TextRange::new(start_pos, end_pos);
        let mut diagnostic = Diagnostic::new(TooManyNewlinesAtEndOfFile, range);
        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start_pos, end_pos)));
        return Some(diagnostic);
    }

    None
}
