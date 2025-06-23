use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StringLiteral;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix, Locator};

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

        let first_char = checker
            .locator()
            .slice(TextRange::at(string.start(), TextSize::new(1)));
        let u_position = u32::from(!(first_char == "u" || first_char == "U"));
        let prefix_range =
            TextRange::at(string.start() + TextSize::new(u_position), TextSize::new(1));

        diagnostic.set_fix(convert_u_string_to_regular_string(
            prefix_range,
            string.range(),
            checker.locator(),
        ));
    }
}

/// Generate a [`Fix`] to rewrite an unicode-prefixed string as a regular string.
fn convert_u_string_to_regular_string(
    prefix_range: TextRange,
    node_range: TextRange,
    locator: &Locator,
) -> Fix {
    // Extract the string body.
    let mut content = locator
        .slice(TextRange::new(prefix_range.end(), node_range.end()))
        .to_owned();

    // If the preceding character is equivalent to the quote character, insert a space to avoid a
    // syntax error. For example, when removing the `u` prefix in `""u""`, rewrite to `"" ""`
    // instead of `""""`.
    // see https://github.com/astral-sh/ruff/issues/18895
    if locator
        .slice(TextRange::up_to(prefix_range.start()))
        .chars()
        .last()
        .is_some_and(|char| content.starts_with(char))
    {
        content.insert(0, ' ');
    }

    Fix::safe_edit(Edit::replacement(
        content,
        prefix_range.start(),
        node_range.end(),
    ))
}
