use crate::autofix::actions::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use log::error;
use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::RefEquality;
use rustpython_parser::ast::{ExprConstant, ExprKind, Stmt, StmtExpr, StmtKind};

/// PYI013
/// ## What it does
/// Removes `...` in otherwise non-empty class bodies
///
/// ## Why is this bad?
/// The `...` is unnecessary and harms readability
///
///
/// ## Example
/// ```python
/// class MyClass:
//     ...
//     value: int
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
//     value: int
/// ```
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

pub(crate) fn ellipsis_in_non_empty_class_body<'a>(
    checker: &mut Checker<'a>,
    parent: &'a Stmt,
    body: &'a [Stmt],
) {
    // If body contains only one statement then it's okay for it to potentially be an Ellipsis
    if body.len() < 2 {
        return;
    }

    for stmt in body {
        if let StmtKind::Expr(StmtExpr { value }) = &stmt.node {
            if let ExprKind::Constant(ExprConstant { value, .. }) = &value.node {
                if value.is_ellipsis() {
                    let mut diagnostic = Diagnostic::new(EllipsisInNonEmptyClassBody, stmt.range());

                    if checker.patch(diagnostic.kind.rule()) {
                        let deleted: Vec<&Stmt> =
                            checker.deletions.iter().map(Into::into).collect();
                        match delete_stmt(
                            stmt,
                            Some(parent),
                            &deleted,
                            checker.locator,
                            checker.indexer,
                            checker.stylist,
                        ) {
                            Ok(edit) => {
                                // In the unlikely event the class body consists solely of several
                                // consecutive ellipses, `delete_stmt` can actually result in a
                                // `pass`
                                if edit.is_deletion() || edit.content() == Some("pass") {
                                    checker.deletions.insert(RefEquality(stmt));
                                    diagnostic.set_fix(Fix::automatic(edit));
                                }
                            }
                            Err(e) => {
                                error!("Failed to delete `...` statement: {}", e);
                            }
                        };
                    };

                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}
