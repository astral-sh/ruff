use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for f-strings that do not contain any placeholder expressions.
///
/// ## Why is this bad?
/// f-strings are a convenient way to format strings, but they are not
/// necessary if there are no placeholder expressions to format. In this
/// case, a regular string should be used instead, as an f-string without
/// placeholders can be confusing for readers, who may expect such a
/// placeholder to be present.
///
/// An f-string without any placeholders could also indicate that the
/// author forgot to add a placeholder expression.
///
/// ## Example
/// ```python
/// f"Hello, world!"
/// ```
///
/// Use instead:
/// ```python
/// "Hello, world!"
/// ```
///
/// **Note:** to maintain compatibility with PyFlakes, this rule only flags
/// f-strings that are part of an implicit concatenation if _none_ of the
/// f-string segments contain placeholder expressions.
///
/// For example:
///
/// ```python
/// # Will not be flagged.
/// (
///     f"Hello,"
///     f" {name}!"
/// )
///
/// # Will be flagged.
/// (
///     f"Hello,"
///     f" World!"
/// )
/// ```
///
/// See [#10885](https://github.com/astral-sh/ruff/issues/10885) for more.
///
/// ## References
/// - [PEP 498](https://www.python.org/dev/peps/pep-0498/)
#[violation]
pub struct FStringMissingPlaceholders;

impl AlwaysFixableViolation for FStringMissingPlaceholders {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string without any placeholders")
    }

    fn fix_title(&self) -> String {
        "Remove extraneous `f` prefix".to_string()
    }
}

/// F541
pub(crate) fn f_string_missing_placeholders(checker: &mut Checker, expr: &ast::ExprFString) {
    if expr.value.f_strings().any(|f_string| {
        f_string
            .elements
            .iter()
            .any(ast::FStringElement::is_expression)
    }) {
        return;
    }

    for f_string in expr.value.f_strings() {
        let first_char = checker
            .locator()
            .slice(TextRange::at(f_string.start(), TextSize::new(1)));
        // f"..."  => f_position = 0
        // fr"..." => f_position = 0
        // rf"..." => f_position = 1
        let f_position = u32::from(!(first_char == "f" || first_char == "F"));
        let prefix_range = TextRange::at(
            f_string.start() + TextSize::new(f_position),
            TextSize::new(1),
        );

        let mut diagnostic = Diagnostic::new(FStringMissingPlaceholders, f_string.range());
        diagnostic.set_fix(convert_f_string_to_regular_string(
            prefix_range,
            f_string.range(),
            checker.locator(),
        ));
        checker.diagnostics.push(diagnostic);
    }
}

/// Unescape an f-string body by replacing `{{` with `{` and `}}` with `}`.
///
/// In Python, curly-brace literals within f-strings must be escaped by doubling the braces.
/// When rewriting an f-string to a regular string, we need to unescape any curly-brace literals.
///  For example, given `{{Hello, world!}}`, return `{Hello, world!}`.
fn unescape_f_string(content: &str) -> String {
    content.replace("{{", "{").replace("}}", "}")
}

/// Generate a [`Fix`] to rewrite an f-string as a regular string.
fn convert_f_string_to_regular_string(
    prefix_range: TextRange,
    node_range: TextRange,
    locator: &Locator,
) -> Fix {
    // Extract the f-string body.
    let mut content =
        unescape_f_string(locator.slice(TextRange::new(prefix_range.end(), node_range.end())));

    // If the preceding character is equivalent to the quote character, insert a space to avoid a
    // syntax error. For example, when removing the `f` prefix in `""f""`, rewrite to `"" ""`
    // instead of `""""`.
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
