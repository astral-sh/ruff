use rustpython_parser::ast::Location;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
    lineno: usize,
    shebang: &ShebangDirective,
    autofix: bool,
) -> Option<Diagnostic> {
    if let ShebangDirective::Match(n_spaces, start, ..) = shebang {
        if *n_spaces > 0 && *start == n_spaces + 2 {
            let mut diagnostic = Diagnostic::new(
                ShebangLeadingWhitespace,
                Range::new(
                    Location::new(lineno + 1, 0),
                    Location::new(lineno + 1, *n_spaces),
                ),
            );
            if autofix {
                diagnostic.set_fix(Edit::deletion(
                    Location::new(lineno + 1, 0),
                    Location::new(lineno + 1, *n_spaces),
                ));
            }
            Some(diagnostic)
        } else {
            None
        }
    } else {
        None
    }
}
