use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, PySourceType};
use ruff_python_parser::{lexer, AsMode, StringKind, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
/// ## References
/// - [PEP 498](https://www.python.org/dev/peps/pep-0498/)
#[violation]
pub struct FStringMissingPlaceholders;

impl AlwaysAutofixableViolation for FStringMissingPlaceholders {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string without any placeholders")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous `f` prefix".to_string()
    }
}

/// Find f-strings that don't contain any formatted values in an [`FString`].
fn find_useless_f_strings<'a>(
    expr: &'a Expr,
    locator: &'a Locator,
    source_type: PySourceType,
) -> impl Iterator<Item = (TextRange, TextRange)> + 'a {
    let contents = locator.slice(expr);
    lexer::lex_starts_at(contents, source_type.as_mode(), expr.start())
        .flatten()
        .filter_map(|(tok, range)| match tok {
            Tok::String {
                kind: StringKind::FString | StringKind::RawFString,
                ..
            } => {
                let first_char = locator.slice(TextRange::at(range.start(), TextSize::from(1)));
                // f"..."  => f_position = 0
                // fr"..." => f_position = 0
                // rf"..." => f_position = 1
                let f_position = u32::from(!(first_char == "f" || first_char == "F"));
                Some((
                    TextRange::at(
                        range.start() + TextSize::from(f_position),
                        TextSize::from(1),
                    ),
                    range,
                ))
            }
            _ => None,
        })
}

/// F541
pub(crate) fn f_string_missing_placeholders(expr: &Expr, values: &[Expr], checker: &mut Checker) {
    if !values
        .iter()
        .any(|value| matches!(value, Expr::FormattedValue(_)))
    {
        for (prefix_range, tok_range) in
            find_useless_f_strings(expr, checker.locator(), checker.source_type)
        {
            let mut diagnostic = Diagnostic::new(FStringMissingPlaceholders, tok_range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(convert_f_string_to_regular_string(
                    prefix_range,
                    tok_range,
                    checker.locator(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
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
    tok_range: TextRange,
    locator: &Locator,
) -> Fix {
    // Extract the f-string body.
    let mut content =
        unescape_f_string(locator.slice(TextRange::new(prefix_range.end(), tok_range.end())));

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

    Fix::automatic(Edit::replacement(
        content,
        prefix_range.start(),
        tok_range.end(),
    ))
}
