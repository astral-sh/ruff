use rustpython_parser::ast::Location;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::Rule;
use crate::settings::{flags, Settings};

/// ## What it does
/// Checks for superfluous trailing whitespace.
///
/// ## Why is this bad?
/// Per PEP 8, "avoid trailing whitespace anywhere. Because it’s usually
/// invisible, it can be confusing"
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
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#other-recommendations)
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
///
/// ## Why is this bad?
/// Per PEP 8, "avoid trailing whitespace anywhere. Because it’s usually
/// invisible, it can be confusing"
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
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#other-recommendations)
#[violation]
pub struct BlankLineWithWhitespace;

impl AlwaysAutofixableViolation for BlankLineWithWhitespace {
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
            if settings.rules.enabled(Rule::BlankLineWithWhitespace) {
                let mut diagnostic =
                    Diagnostic::new(BlankLineWithWhitespace, Range::new(start, end));
                if matches!(autofix, flags::Autofix::Enabled)
                    && settings.rules.should_fix(Rule::BlankLineWithWhitespace)
                {
                    diagnostic.set_fix(Edit::deletion(start, end));
                }
                return Some(diagnostic);
            }
        } else if settings.rules.enabled(Rule::TrailingWhitespace) {
            let mut diagnostic = Diagnostic::new(TrailingWhitespace, Range::new(start, end));
            if matches!(autofix, flags::Autofix::Enabled)
                && settings.rules.should_fix(Rule::TrailingWhitespace)
            {
                diagnostic.set_fix(Edit::deletion(start, end));
            }
            return Some(diagnostic);
        }
    }
    None
}
