use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_whitespace::Line;

use crate::registry::Rule;
use crate::settings::Settings;

/// ## What it does
/// Checks for superfluous trailing whitespace.
///
/// ## Why is this bad?
/// According to [PEP 8], "avoid trailing whitespace anywhere. Because it’s usually
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
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
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
/// According to [PEP 8], "avoid trailing whitespace anywhere. Because it’s usually
/// invisible, it can be confusing"
///
/// ## Example
/// ```python
/// class Foo(object):\n    \n    bang = 12
/// ```
///
/// Use instead:
/// ```python
/// class Foo(object):\n\n    bang = 12
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
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
pub(crate) fn trailing_whitespace(
    line: &Line,
    prev_line: &Option<Line>,
    settings: &Settings,
) -> Option<Diagnostic> {
    let whitespace_len: TextSize = line
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .map(TextLen::text_len)
        .sum();
    if whitespace_len > TextSize::from(0) {
        let range = TextRange::new(line.end() - whitespace_len, line.end());

        if range == line.range() {
            if settings.rules.enabled(Rule::BlankLineWithWhitespace) {
                let mut diagnostic = Diagnostic::new(BlankLineWithWhitespace, range);

                if settings.rules.should_fix(Rule::BlankLineWithWhitespace) {
                    // If this line is blank with whitespace, we have to ensure that the previous line
                    // doesn't end with a backslash. If it did, the file would end with a backslash
                    // and therefore have an "unexpected EOF" SyntaxError, so we have to remove it.
                    if let Some(prev) = prev_line {
                        let trimmed = prev.trim_end();
                        if trimmed.ends_with('\\') {
                            let initial_len = prev.text_len();
                            diagnostic.range = range.sub_start(
                                // Shift by the amount of whitespace in the previous line, plus the
                                // newline, plus the slash, plus any remaining whitespace after it.
                                initial_len - trimmed.text_len()
                                    + TextSize::from(1)
                                    + '\\'.text_len()
                                    + (trimmed.text_len() - trimmed.trim_end().text_len()
                                        + TextSize::from(1)),
                            );
                        }
                    }

                    diagnostic.set_fix(Fix::suggested(Edit::range_deletion(diagnostic.range)));
                }

                return Some(diagnostic);
            }
        } else if settings.rules.enabled(Rule::TrailingWhitespace) {
            let mut diagnostic = Diagnostic::new(TrailingWhitespace, range);
            if settings.rules.should_fix(Rule::TrailingWhitespace) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_deletion(range)));
            }
            return Some(diagnostic);
        }
    }
    None
}
