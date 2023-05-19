use rustpython_parser::ast::{Expr, ExprConstant, Ranged, Stmt, StmtExpr};

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::RefEquality;

use crate::autofix::actions::delete_stmt;
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
        if let Stmt::Expr(StmtExpr { value, .. }) = &stmt {
            if let Expr::Constant(ExprConstant { value, .. }) = value.as_ref() {
                if value.is_ellipsis() {
                    let mut diagnostic = Diagnostic::new(EllipsisInNonEmptyClassBody, stmt.range());

                    if checker.patch(diagnostic.kind.rule()) {
                        diagnostic.try_set_fix(|| {
                            let deleted: Vec<&Stmt> =
                                checker.deletions.iter().map(Into::into).collect();
                            let edit = delete_stmt(
                                stmt,
                                Some(parent),
                                &deleted,
                                checker.locator,
                                checker.indexer,
                                checker.stylist,
                            )?;

                            // In the unlikely event the class body consists solely of several
                            // consecutive ellipses, `delete_stmt` can actually result in a
                            // `pass`.
                            if edit.is_deletion() || edit.content() == Some("pass") {
                                checker.deletions.insert(RefEquality(stmt));
                            }

                            Ok(Fix::automatic(edit))
                        });
                    }

                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}
