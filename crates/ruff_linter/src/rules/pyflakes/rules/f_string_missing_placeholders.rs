use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::traversal::{self, EnclosingSuite};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::ScopeKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// ## Fix safety
/// The fix is marked unsafe when the f-string sits in a docstring position, because removing
/// the `f` prefix turns it into a string literal and the program starts exposing it as
/// documentation. This covers the first statement of a module, class or function body, and a
/// statement following a simple assignment at module level or in a class body:
///
/// ```python
/// f"Not a docstring."
///
/// a = 1
/// f"Not an attribute docstring."
/// ```
///
/// ## References
/// - [PEP 498 – Literal String Interpolation](https://peps.python.org/pep-0498/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.18", category = Category::Complexity)]
pub(crate) struct FStringMissingPlaceholders;

impl AlwaysFixableViolation for FStringMissingPlaceholders {
    #[derive_message_formats]
    fn message(&self) -> String {
        "f-string without any placeholders".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove extraneous `f` prefix".to_string()
    }
}

/// F541
pub(crate) fn f_string_missing_placeholders(checker: &Checker, expr: &ast::ExprFString) {
    if expr.value.f_strings().any(|f_string| {
        f_string
            .elements
            .iter()
            .any(ast::InterpolatedStringElement::is_interpolation)
    }) {
        return;
    }

    let applicability = if fix_creates_docstring(checker, expr) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

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

        let mut diagnostic =
            checker.report_diagnostic(FStringMissingPlaceholders, f_string.range());
        diagnostic.set_fix(convert_f_string_to_regular_string(
            prefix_range,
            f_string.range(),
            checker.locator(),
            applicability,
        ));
    }
}

/// Returns `true` if dropping the `f` prefix would place a string literal in a docstring
/// position: the first statement of a module, class or function body, or the statement
/// following a simple assignment at module level or in a class body.
fn fix_creates_docstring(checker: &Checker, expr: &ast::ExprFString) -> bool {
    let semantic = checker.semantic();
    let stmt = semantic.current_statement();

    // The f-string has to make up the entire statement; a nested one never becomes a docstring.
    let Some(ast::StmtExpr { value, .. }) = stmt.as_expr_stmt() else {
        return false;
    };
    if value.range() != expr.range() {
        return false;
    }

    let at_top_level = semantic.at_top_level();
    let parent = semantic.current_statement_parent();

    let suite = if at_top_level {
        // At module level there is no parent statement, so take the body from the definitions.
        semantic
            .definitions
            .python_ast()
            .and_then(|body| EnclosingSuite::new(body, stmt.into()))
    } else {
        parent.and_then(|parent| traversal::suite(stmt, parent))
    };
    let Some(suite) = suite else {
        return false;
    };

    let in_docstring_body = match parent {
        None => at_top_level,
        Some(Stmt::FunctionDef(_) | Stmt::ClassDef(_)) => true,
        Some(_) => false,
    };
    if in_docstring_body && suite.first() == Some(stmt) {
        return true;
    }

    // Attribute docstrings follow a simple assignment, at module level or in a class body.
    if at_top_level || matches!(semantic.current_scope().kind, ScopeKind::Class(_)) {
        match suite.previous_sibling() {
            Some(Stmt::Assign(ast::StmtAssign { targets, .. })) => {
                return matches!(targets.as_slice(), [Expr::Name(_)]);
            }
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { target, .. })) => {
                return target.is_name_expr();
            }
            _ => {}
        }
    }

    false
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
    applicability: Applicability,
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

    Fix::applicable_edit(
        Edit::replacement(content, prefix_range.start(), node_range.end()),
        applicability,
    )
}
