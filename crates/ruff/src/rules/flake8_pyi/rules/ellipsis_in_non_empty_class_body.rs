use rustpython_parser::ast::{Expr, ExprConstant, Ranged, Stmt, StmtExpr};

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Removes ellipses (`...`) in otherwise non-empty class bodies.
///
/// ## Why is this bad?
/// An ellipsis in a class body is only necessary if the class body is
/// otherwise empty. If the class body is non-empty, then the ellipsis
/// is redundant.
///
/// ## Example
/// ```python
/// class Foo:
///     ...
///     value: int
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     value: int
/// ```
#[violation]
pub struct EllipsisInNonEmptyClassBody;

impl Violation for EllipsisInNonEmptyClassBody {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Non-empty class body must not contain `...`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Remove unnecessary `...`".to_string())
    }
}

/// PYI013
pub(crate) fn ellipsis_in_non_empty_class_body<'a>(
    checker: &mut Checker<'a>,
    parent: &'a Stmt,
    body: &'a [Stmt],
) {
    // If the class body contains a single statement, then it's fine for it to be an ellipsis.
    if body.len() == 1 {
        return;
    }

    for stmt in body {
        let Stmt::Expr(StmtExpr { value, .. }) = &stmt else {
            continue;
        };

        let Expr::Constant(ExprConstant { value, .. }) = value.as_ref() else {
            continue;
        };

        if !value.is_ellipsis() {
            continue;
        }

        let mut diagnostic = Diagnostic::new(EllipsisInNonEmptyClassBody, stmt.range());
        if checker.patch(diagnostic.kind.rule()) {
            let edit =
                autofix::edits::delete_stmt(stmt, Some(parent), checker.locator, checker.indexer);
            diagnostic.set_fix(Fix::automatic(edit).isolate(checker.isolation(Some(parent))));
        }
        checker.diagnostics.push(diagnostic);
    }
}
