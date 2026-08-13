use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;
use crate::fix;
use crate::preview::is_pyi048_fix_enabled;
use crate::rules::flake8_pie::rules::Placeholder;
use crate::{Applicability, Fix, FixAvailability, Violation};

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
/// The fix removes placeholder statements (`...` and `pass`), which do nothing at runtime, so it is
/// only available when every statement but one is a placeholder. It also requires [preview] to be
/// enabled.
///
/// ## Fix safety
///
/// As in [`unnecessary-placeholder` (`PIE790`)][PIE790], the fix is marked as unsafe when the
/// surviving statement is a string literal that isn't already the docstring, since removing the
/// placeholders that precede it turns it into one:
///
/// ```pyi
/// def function():
///     ...
///     "this string becomes the docstring"
/// ```
///
/// [PIE790]: https://docs.astral.sh/ruff/rules/unnecessary-placeholder/
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271")]
pub(crate) struct StubBodyMultipleStatements;

impl Violation for StubBodyMultipleStatements {
    // Sometimes fixable, and only under preview.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Function body must contain exactly one statement".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unnecessary placeholder statements".to_string())
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

/// Build a [`Fix`] that deletes every placeholder statement in `body` but one, if that leaves the
/// body with a single statement.
fn remove_placeholders(checker: &Checker, body: &[Stmt]) -> Option<Fix> {
    let mut non_placeholders = body
        .iter()
        .enumerate()
        .filter(|(_, stmt)| Placeholder::from_stmt(stmt).is_none());

    let mut applicability = Applicability::Safe;

    let keep = match (non_placeholders.next(), non_placeholders.next()) {
        (Some((index, stmt)), None) => {
            // Deleting the placeholders that precede a string literal turns it into the function's
            // docstring, so the fix is unsafe unless the literal already is the docstring. This
            // matches how `PIE790` treats the same situation.
            if index > 0
                && stmt
                    .as_expr_stmt()
                    .is_some_and(|stmt| stmt.value.is_string_literal_expr())
            {
                applicability = Applicability::Unsafe;
            }
            index
        }
        // The body is nothing but placeholders, and one of them has to stay, since a body can't be
        // empty. Keep an ellipsis if the body has one, so that the result is the `...` body a stub
        // is supposed to have; otherwise keep the first `pass`, which `PYI009` then rewrites.
        (None, _) => body
            .iter()
            .position(|stmt| matches!(Placeholder::from_stmt(stmt), Some(Placeholder::Ellipsis)))
            .unwrap_or(0),
        // More than one statement would be left behind, which wouldn't resolve the rule violation,
        // so there's nothing to fix.
        (Some(_), Some(_)) => return None,
    };

    let mut edits = body
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
        });

    let first = edits.next()?;

    Some(
        Fix::applicable_edits(first, edits, applicability).isolate(Checker::isolation(
            checker.semantic().current_statement_id(),
        )),
    )
}
