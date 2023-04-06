use rustpython_parser::ast::{Expr, ExprKind, Location};
use rustpython_parser::{lexer, Mode, StringKind, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for f-strings that do not contain any placeholder expressions.
///
/// ## Why is this bad?
/// F-strings are a convenient way to format strings, but they are not
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

/// Find f-strings that don't contain any formatted values in a `JoinedStr`.
fn find_useless_f_strings<'a>(
    expr: &'a Expr,
    locator: &'a Locator,
) -> impl Iterator<Item = (Range, Range)> + 'a {
    let contents = locator.slice(expr);
    lexer::lex_located(contents, Mode::Module, expr.location)
        .flatten()
        .filter_map(|(location, tok, end_location)| match tok {
            Tok::String {
                kind: StringKind::FString | StringKind::RawFString,
                ..
            } => {
                let first_char = locator.slice(Range {
                    location,
                    end_location: Location::new(location.row(), location.column() + 1),
                });
                // f"..."  => f_position = 0
                // fr"..." => f_position = 0
                // rf"..." => f_position = 1
                let f_position = usize::from(!(first_char == "f" || first_char == "F"));
                Some((
                    Range {
                        location: Location::new(location.row(), location.column() + f_position),
                        end_location: Location::new(
                            location.row(),
                            location.column() + f_position + 1,
                        ),
                    },
                    Range {
                        location,
                        end_location,
                    },
                ))
            }
            _ => None,
        })
}

fn unescape_f_string(content: &str) -> String {
    content.replace("{{", "{").replace("}}", "}")
}

fn fix_f_string_missing_placeholders(
    prefix_range: &Range,
    tok_range: &Range,
    checker: &mut Checker,
) -> Edit {
    let content = checker.locator.slice(Range::new(
        prefix_range.end_location,
        tok_range.end_location,
    ));
    Edit::replacement(
        unescape_f_string(content),
        prefix_range.location,
        tok_range.end_location,
    )
}

/// F541
pub fn f_string_missing_placeholders(expr: &Expr, values: &[Expr], checker: &mut Checker) {
    if !values
        .iter()
        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
    {
        for (prefix_range, tok_range) in find_useless_f_strings(expr, checker.locator) {
            let mut diagnostic = Diagnostic::new(FStringMissingPlaceholders, tok_range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(fix_f_string_missing_placeholders(
                    &prefix_range,
                    &tok_range,
                    checker,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
