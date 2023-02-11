use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::flake8_executable::helpers::ShebangDirective;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ShebangWhitespace;
);
impl AlwaysAutofixableViolation for ShebangWhitespace {
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
                ShebangWhitespace,
                Range::new(
                    Location::new(lineno + 1, 0),
                    Location::new(lineno + 1, *n_spaces),
                ),
            );
            if autofix {
                diagnostic.amend(Fix::deletion(
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
