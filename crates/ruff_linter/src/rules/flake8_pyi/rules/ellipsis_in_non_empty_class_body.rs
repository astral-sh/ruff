use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::whitespace::trailing_comment_start_offset;
use ruff_python_ast::{Stmt, StmtExpr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Removes ellipses (`...`) in otherwise non-empty class bodies.
///
/// ## Why is this bad?
/// An ellipsis in a class body is only necessary if the class body is
/// otherwise empty. If the class body is non-empty, then the ellipsis
/// is redundant.
///
/// ## Example
/// ```pyi
/// class Foo:
///     ...
///     value: int
/// ```
///
/// Use instead:
/// ```pyi
/// class Foo:
///     value: int
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.270")]
pub(crate) struct EllipsisInNonEmptyClassBody;

impl Violation for EllipsisInNonEmptyClassBody {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Non-empty class body must not contain `...`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unnecessary `...`".to_string())
    }
}

/// PYI013
pub(crate) fn ellipsis_in_non_empty_class_body(checker: &Checker, body: &[Stmt]) {
    // If the class body contains a single statement, then it's fine for it to be an ellipsis.
    if body.len() == 1 {
        return;
    }

    for stmt in body {
        let Stmt::Expr(StmtExpr { value, .. }) = stmt else {
            continue;
        };

        if value.is_ellipsis_literal_expr() {
            let mut diagnostic =
                checker.report_diagnostic(EllipsisInNonEmptyClassBody, stmt.range());

            // Try to preserve trailing comment if it exists
            let edit = if let Some(index) = trailing_comment_start_offset(stmt, checker.source()) {
                Edit::range_deletion(stmt.range().add_end(index))
            } else {
                fix::edits::delete_stmt(stmt, Some(stmt), checker.locator(), checker.indexer())
            };

            diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
                checker.semantic().current_statement_id(),
            )));
        }
    }
}
