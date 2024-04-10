use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::{Line, Locator};
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::registry::Rule;
use crate::settings::LinterSettings;

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

impl AlwaysFixableViolation for TrailingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing whitespace")
    }

    fn fix_title(&self) -> String {
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

impl AlwaysFixableViolation for BlankLineWithWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Blank line contains whitespace")
    }

    fn fix_title(&self) -> String {
        "Remove whitespace from blank line".to_string()
    }
}

/// W291, W293
pub(crate) fn trailing_whitespace(
    line: &Line,
    locator: &Locator,
    indexer: &Indexer,
    settings: &LinterSettings,
) -> Option<Diagnostic> {
    let whitespace_len: TextSize = line
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .map(TextLen::text_len)
        .sum();
    if whitespace_len > TextSize::from(0) {
        let range = TextRange::new(line.end() - whitespace_len, line.end());
        // Removing trailing whitespace is not safe inside multiline strings.
        let applicability = if indexer.multiline_ranges().contains_range(range) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };
        if range == line.range() {
            if settings.rules.enabled(Rule::BlankLineWithWhitespace) {
                let mut diagnostic = Diagnostic::new(BlankLineWithWhitespace, range);
                // Remove any preceding continuations, to avoid introducing a potential
                // syntax error.
                diagnostic.set_fix(Fix::applicable_edit(
                    Edit::range_deletion(TextRange::new(
                        indexer
                            .preceded_by_continuations(line.start(), locator)
                            .unwrap_or(range.start()),
                        range.end(),
                    )),
                    applicability,
                ));
                return Some(diagnostic);
            }
        } else if settings.rules.enabled(Rule::TrailingWhitespace) {
            let mut diagnostic = Diagnostic::new(TrailingWhitespace, range);
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_deletion(range),
                applicability,
            ));
            return Some(diagnostic);
        }
    }
    None
}
