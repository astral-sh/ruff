use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Stmt, StmtExpr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

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
        let Stmt::Expr(StmtExpr { value, .. }) = &stmt else {
            continue;
        };

        if value.is_ellipsis_literal_expr() {
            let mut diagnostic = Diagnostic::new(EllipsisInNonEmptyClassBody, stmt.range());
            let edit =
                fix::edits::delete_stmt(stmt, Some(stmt), checker.locator(), checker.indexer());
            diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
                checker.semantic().current_statement_id(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}
