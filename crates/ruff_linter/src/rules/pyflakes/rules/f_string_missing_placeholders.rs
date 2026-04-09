use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for f-strings that do not contain any placeholder expressions.
///
/// ## Why is this bad?
/// f-strings are a convenient way to format strings, but they are not
/// necessary if the string does not contain any placeholder expressions.
///
/// In some cases, the `f` prefix may have been added unintentionally;
/// in others, placeholder expressions may have been removed or omitted
/// during development.
///
/// For compatibility with [`typing.LiteralString`][typing.LiteralString], use a plain
/// string literal instead of an f-string with no placeholders.
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
/// ## Fix safety
/// This rule's fix is marked as [safe] when the f-string is not located in
/// a position that would make the resulting plain string a docstring.
/// Otherwise, the fix is omitted entirely to avoid changing the runtime
/// semantics of the code.
///
/// [typing.LiteralString]: https://docs.python.org/3/library/typing.html#typing.LiteralString
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.18")]
pub(crate) struct FStringMissingPlaceholders;

impl Violation for FStringMissingPlaceholders {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "f-string without any placeholders".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove extraneous `f` prefix".to_string())
    }
}

/// F541
pub(crate) fn f_string_missing_placeholders(checker: &Checker, expr: &ast::ExprFString) {
    if expr
        .value
        .f_strings()
        .any(|f_string| {
            f_string
                .elements
                .iter()
                .any(ast::InterpolatedStringElement::is_interpolation)
        })
    {
        return;
    }

    let in_docstring_position = would_fix_create_docstring(checker, expr);

    for f_string in expr.value.f_strings() {
        let first_char =
            checker
                .locator()
                .slice(TextRange::at(f_string.start(), TextSize::new(1)));
        let f_position = u32::from(!(first_char == "f" || first_char == "F"));
        let prefix_range =
            TextRange::at(f_string.start() + TextSize::new(f_position), TextSize::new(1));

        let mut diagnostic =
            checker.report_diagnostic(FStringMissingPlaceholders, f_string.range());

        if !in_docstring_position {
            diagnostic.set_fix(convert_f_string_to_regular_string(
                prefix_range,
                f_string.range(),
                checker.locator(),
            ));
        }
    }
}

/// Returns `true` if removing the `f` prefix from the given f-string
/// expression would turn it into a docstring (i.e., the expression is the
/// first statement in a function, class, or module body).
fn would_fix_create_docstring(checker: &Checker, expr: &ast::ExprFString) -> bool {
    let semantic = checker.semantic();
    let stmt = semantic.current_statement();

    // The expression must be a standalone expression statement whose value
    // is the f-string we are checking.
    let ast::Stmt::Expr(stmt_expr) = stmt else {
        return false;
    };
    if !matches!(stmt_expr.value.as_ref(), ast::Expr::FString(_)) {
        return false;
    }

    // Check whether this statement is the first in a function/class body
    // or at module level.
    match semantic.current_statement_parent() {
        Some(ast::Stmt::FunctionDef(func)) => func
            .body
            .first()
            .is_some_and(|first| first.range() == stmt.range()),
        Some(ast::Stmt::ClassDef(class)) => class
            .body
            .first()
            .is_some_and(|first| first.range() == stmt.range()),
        None => {
            // Module-level: this is the first non-comment, non-blank line.
            let before = checker
                .locator()
                .slice(TextRange::up_to(stmt.start()));
            !before
                .lines()
                .any(|line| {
                    let trimmed = line.trim();
                    !trimmed.is_empty() && !trimmed.starts_with('#')
                })
        }
        _ => false,
    }
}

/// Unescape an f-string body by replacing `{{` with `{` and `}}` with `}`.
fn unescape_f_string(content: &str) -> String {
    content.replace("{{", "{").replace("}}", "}")
}

/// Generate a [`Fix`] to rewrite an f-string as a regular string.
fn convert_f_string_to_regular_string(
    prefix_range: TextRange,
    node_range: TextRange,
    locator: &Locator,
) -> Fix {
    let mut content = unescape_f_string(
        locator.slice(TextRange::new(prefix_range.end(), node_range.end())),
    );

    // If the preceding character matches the opening quote, add a space to
    // avoid merging adjacent string literals.
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
