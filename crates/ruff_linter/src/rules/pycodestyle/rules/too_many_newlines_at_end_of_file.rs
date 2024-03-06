use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;

#[violation]
pub struct TooManyNewlinesAtEndOfFile;

impl AlwaysFixableViolation for TooManyNewlinesAtEndOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Too many newlines at the end of file".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove extraneous trailing newlines".to_string()
    }
}

/// W391
pub(crate) fn too_many_newlines_at_end_of_file(
    locator: &Locator,
) -> Option<Diagnostic> {
    let source = locator.contents();

    // Ignore empty and BOM only files
    if source.is_empty() || source == "\u{feff}" {
        return None;
    }

    // Regex to match multiple newline characters at the end of the file
    let newline_regex = Regex::new(r"(\n|\r\n){2,}$").unwrap();

    if let Some(mat) = newline_regex.find(source) {
        let start_pos = mat.start();
        let end_pos = mat.end();

        // Calculate the TextRange to keep only one newline at the end
        let range = TextRange::new(
            TextLen::from(start_pos as u32 + 1),  // Keep one newline
            TextLen::from(end_pos as u32),
        );

        let mut diagnostic = Diagnostic::new(TooManyNewlinesAtEndOfFile, range);
        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(range)));
        return Some(diagnostic);
    }

    None
}
