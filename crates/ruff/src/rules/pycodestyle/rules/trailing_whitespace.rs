use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct TrailingWhitespace;
);
impl AlwaysAutofixableViolation for TrailingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing whitespace")
    }

    fn autofix_title(&self) -> String {
        "Remove trailing whitespace".to_string()
    }
}

define_violation!(
    pub struct BlankLineContainsWhitespace;
);
impl AlwaysAutofixableViolation for BlankLineContainsWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Blank line contains whitespace")
    }

    fn autofix_title(&self) -> String {
        "Remove whitespace from blank line".to_string()
    }
}

/// W291, W293
pub fn trailing_whitespace(
    lineno: usize,
    line: &str,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Diagnostic> {
    let whitespace_count = line.chars().rev().take_while(|c| c.is_whitespace()).count();
    if whitespace_count > 0 {
        let line_char_count = line.chars().count();
        let start = Location::new(lineno + 1, line_char_count - whitespace_count);
        let end = Location::new(lineno + 1, line_char_count);

        if whitespace_count == line_char_count {
            if settings.rules.enabled(&Rule::BlankLineContainsWhitespace) {
                let mut diagnostic =
                    Diagnostic::new(BlankLineContainsWhitespace, Range::new(start, end));
                if matches!(autofix, flags::Autofix::Enabled)
                    && settings
                        .rules
                        .should_fix(&Rule::BlankLineContainsWhitespace)
                {
                    diagnostic.amend(Fix::deletion(start, end));
                }
                return Some(diagnostic);
            }
        } else if settings.rules.enabled(&Rule::TrailingWhitespace) {
            let mut diagnostic = Diagnostic::new(TrailingWhitespace, Range::new(start, end));
            if matches!(autofix, flags::Autofix::Enabled)
                && settings.rules.should_fix(&Rule::TrailingWhitespace)
            {
                diagnostic.amend(Fix::deletion(start, end));
            }
            return Some(diagnostic);
        }
    }
    None
}
