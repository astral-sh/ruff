use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StringLiteral;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of the Unicode kind prefix (`u`) in strings.
///
/// ## Why is this bad?
/// In Python 3, all strings are Unicode by default. The Unicode kind prefix is
/// unnecessary and should be removed to avoid confusion.
///
/// ## Example
/// ```python
/// u"foo"
/// ```
///
/// Use instead:
/// ```python
/// "foo"
/// ```
///
/// ## References
/// - [Python documentation: Unicode HOWTO](https://docs.python.org/3/howto/unicode.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.201")]
pub(crate) struct UnicodeKindPrefix;

impl AlwaysFixableViolation for UnicodeKindPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Remove unicode literals from strings".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }
}

/// UP025
pub(crate) fn unicode_kind_prefix(checker: &Checker, string: &StringLiteral) {
    if string.flags.prefix().is_unicode() {
        let mut diagnostic = checker.report_diagnostic(UnicodeKindPrefix, string.range);

        let prefix_range = TextRange::at(string.start(), TextSize::new(1));
        let locator = checker.locator();
        let content = locator
            .slice(TextRange::new(prefix_range.end(), string.end()))
            .to_owned();

        // If the preceding character is equivalent to the quote character, insert a space to avoid a
        // syntax error. For example, when removing the `u` prefix in `""u""`, rewrite to `"" ""`
        // instead of `""""`.
        // see https://github.com/astral-sh/ruff/issues/18895
        let edit = if locator
            .slice(TextRange::up_to(prefix_range.start()))
            .chars()
            .last()
            .is_some_and(|char| content.starts_with(char))
        {
            Edit::range_replacement(" ".to_string(), prefix_range)
        } else {
            Edit::range_deletion(prefix_range)
        };

        diagnostic.set_fix(Fix::safe_edit(edit));
    }
}
