use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;

/// ## What it does
/// Checks if a file name contains a non-ASCII character.
///
/// ## Why is this bad?
/// Non-ASCII characters in file names can cause problems
/// on some systems or tools.
#[violation]
pub struct NonAsciiFileName;

impl Violation for NonAsciiFileName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("File name contains a non-ASCII character, consider renaming it.")
    }
}

/// PLW2402
pub(crate) fn non_ascii_file_name(path: &Path) -> Option<Diagnostic> {
    if let Some(name) = path.file_name() {
        if let Some(name) = name.to_str() {
            if name.is_ascii() {
                return None;
            }
            return Some(Diagnostic::new(NonAsciiFileName, TextRange::default()));
        }
    }

    None
}
