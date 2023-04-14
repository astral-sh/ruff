use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::flake8_executable::helpers::ShebangDirective;

#[violation]
pub struct ShebangMissingPython;

impl Violation for ShebangMissingPython {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang should contain `python`")
    }
}

/// EXE003
pub fn shebang_python(range: TextRange, shebang: &ShebangDirective) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, content) = shebang {
        if content.contains("python") || content.contains("pytest") {
            None
        } else {
            let diagnostic = Diagnostic::new(
                ShebangMissingPython,
                TextRange::at(range.start() + start, content.text_len())
                    .sub_start(TextSize::from(2)),
            );

            Some(diagnostic)
        }
    } else {
        None
    }
}
