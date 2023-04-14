use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::flake8_executable::helpers::ShebangDirective;

#[violation]
pub struct ShebangLeadingWhitespace;

impl AlwaysAutofixableViolation for ShebangLeadingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid whitespace before shebang")
    }

    fn autofix_title(&self) -> String {
        format!("Remove whitespace before shebang")
    }
}

/// EXE004
pub fn shebang_whitespace(
    range: TextRange,
    shebang: &ShebangDirective,
    autofix: bool,
) -> Option<Diagnostic> {
    if let ShebangDirective::Match(n_spaces, start, ..) = shebang {
        if *n_spaces > TextSize::from(0) && *start == n_spaces + TextSize::from(2) {
            let mut diagnostic = Diagnostic::new(
                ShebangLeadingWhitespace,
                TextRange::at(range.start(), *n_spaces),
            );
            if autofix {
                diagnostic.set_fix(Edit::range_deletion(TextRange::at(
                    range.start(),
                    *n_spaces,
                )));
            }
            Some(diagnostic)
        } else {
            None
        }
    } else {
        None
    }
}
