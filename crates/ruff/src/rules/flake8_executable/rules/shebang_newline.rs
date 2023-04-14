use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::flake8_executable::helpers::ShebangDirective;

#[violation]
pub struct ShebangNotFirstLine;

impl Violation for ShebangNotFirstLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang should be at the beginning of the file")
    }
}

/// EXE005
pub fn shebang_newline(
    range: TextRange,
    shebang: &ShebangDirective,
    first_line: bool,
) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, content) = shebang {
        if first_line {
            None
        } else {
            let diagnostic = Diagnostic::new(
                ShebangNotFirstLine,
                TextRange::at(range.start() + start, content.text_len()),
            );
            Some(diagnostic)
        }
    } else {
        None
    }
}
