use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::identifier::Identifier;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix;
use crate::preview::is_pyi048_fix_enabled;
use crate::rules::flake8_pie::rules::Placeholder;
use crate::{Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for functions in stub (`.pyi`) files that contain multiple
/// statements.
///
/// ## Why is this bad?
/// Stub files are never executed, and are only intended to define type hints.
/// As such, functions in stub files should not contain functional code, and
/// should instead contain only a single statement (e.g., `...`).
///
/// ## Example
///
/// ```pyi
/// def function():
///     x = 1
///     y = 2
///     return x + y
/// ```
///
/// Use instead:
///
/// ```pyi
/// def function(): ...
/// ```
///
/// ## Fix availability
///
/// The fix removes placeholder statements (`...` and `pass`), which do nothing at runtime. It is
/// only available when exactly one statement in the body isn't a placeholder, so that the fix
/// leaves that statement as the body's only one:
///
/// ```pyi
/// def function():
///     """Docstring."""
///     ...
/// ```
///
/// No fix is offered when that statement is a string literal preceded by placeholders, since
/// removing them would turn the string into the function's docstring.
///
/// The fix also requires [preview] to be enabled.
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271", category = Category::Correctness)]
pub(crate) struct StubBodyMultipleStatements;

impl Violation for StubBodyMultipleStatements {
    // Sometimes fixable, and only under preview.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Function body must contain exactly one statement".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unnecessary statements".to_string())
    }
}

/// PYI048
pub(crate) fn stub_body_multiple_statements(checker: &Checker, stmt: &Stmt, body: &[Stmt]) {
    if body.len() <= 1 {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(StubBodyMultipleStatements, stmt.identifier());

    if is_pyi048_fix_enabled(checker.settings())
        && let Some(fix) = remove_placeholders(checker, body)
    {
        diagnostic.set_fix(fix);
    }
}

/// Build a [`Fix`] that deletes every placeholder in `body`, if that leaves the body with a single
/// statement.
fn remove_placeholders(checker: &Checker, body: &[Stmt]) -> Option<Fix> {
    let mut non_placeholders = body
        .iter()
        .enumerate()
        .filter(|(_, stmt)| Placeholder::from_stmt(stmt).is_none());

    let (Some((keep, kept)), None) = (non_placeholders.next(), non_placeholders.next()) else {
        return None;
    };

    // Deleting the placeholders that precede a string literal would turn it into the function's
    // docstring, so no fix is offered.
    if keep > 0 && is_docstring_stmt(kept) {
        return None;
    }

    let edits: Vec<_> = body
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != keep)
        .map(|(_, stmt)| {
            fix::edits::delete_stmt_preserving_trailing_comment(
                stmt,
                None,
                checker.locator(),
                checker.indexer(),
            )
        })
        .collect();

    // The deletions of neighboring statements can overlap when they share a line continuation:
    //
    // ```pyi
    // def f():
    //     "doc"
    //     pass; \
    //     pass
    // ```
    //
    // Deleting the first `pass` extends to the start of the second, while deleting the second
    // starts back at the continuation. A fix can't hold overlapping edits, so offer none.
    if edits.windows(2).any(|pair| pair[0].end() > pair[1].start()) {
        return None;
    }

    let mut edits = edits.into_iter();
    let first = edits.next()?;

    Some(Fix::safe_edits(first, edits).isolate(Checker::isolation(
        checker.semantic().current_statement_id(),
    )))
}
