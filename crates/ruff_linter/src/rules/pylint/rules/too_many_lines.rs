use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::{Locator, UniversalNewlines};
use ruff_text_size::TextRange;

use crate::settings::LinterSettings;

#[violation]
pub struct TooManyLines;

impl Violation for TooManyLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many lines in module")
    }
}

/// PLC0302
pub(crate) fn too_many_lines(locator: &Locator, settings: &LinterSettings) -> Option<Diagnostic> {
    let lines = locator.contents().universal_newlines();
    let length = lines.count() + 1;

    if length > settings.pylint.max_module_lines {
        let diagnostic = Diagnostic::new(TooManyLines, TextRange::default());
        return Some(diagnostic);
    }

    None
}
