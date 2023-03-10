use rustpython_parser::ast::Location;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::Rule;
use crate::settings::{flags, Settings};

/// ## What it does
/// Checks for superfluous trailing whitespace.
/// The warning returned varies on whether the line itself is blank,
/// for easier filtering for those who want to indent their blank lines.
///
/// ## Why is this bad?
///
/// ## Example
/// ```python
/// spam(1) \n#
/// ```
///
/// Use instead:
/// ```python
/// spam(1)\n#
/// ```
#[violation]
pub struct TrailingWhitespace;

impl AlwaysAutofixableViolation for TrailingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing whitespace")
    }

    fn autofix_title(&self) -> String {
        "Remove trailing whitespace".to_string()
    }
}

/// ## What it does
/// Checks for superfluous whitespace in blank lines.
/// The warning returned varies on whether the line itself is blank,
/// for easier filtering for those who want to indent their blank lines.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// class Foo(object):\n    \n    bang = 12
///
/// ```
///
/// Use instead:
/// ```python
/// class Foo(object):\n\n    bang = 12
/// ```
#[violation]
pub struct BlankLineContainsWhitespace;

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
