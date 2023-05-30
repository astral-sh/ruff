use crate::autofix::actions::delete_stmt;
use ruff_diagnostics::Fix;
use ruff_python_ast::types::RefEquality;
use rustpython_parser::ast::Excepthandler;
use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary ellipsis.
///
/// ## Why is this bad?
/// A line of code consisting of an ellipsis is unnecessary if there is a docstring
/// on the preceding line or if there is a statement in the same scope.
///
/// ## Example
/// ```python
/// def my_function():
///     """My docstring"""
///     ...  # [unnecessary-ellipsis]
/// ```
///
/// Use instead:
/// ```python
/// def my_function():
///     """My docstring"""
///     ...  # [unnecessary-ellipsis]
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#lambda)
#[violation]
pub struct UnnecessaryEllipsis;

impl AlwaysAutofixableViolation for UnnecessaryEllipsis {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary ellipsis constant.")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary ellipsis".to_string()
    }
}

fn starts_with_docstring(body: &[Stmt]) -> bool {
    if let Some(Stmt::Expr(ast::StmtExpr { value, .. })) = body.first() {
        if matches!(
            value.as_ref(),
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(_),
                ..
            })
        ) {
            return true;
        }
    };
    false
}

fn is_ellipsis(element: &Stmt) -> bool {
    if let Stmt::Expr(ast::StmtExpr { value, .. }) = element {
        if matches!(
            value.as_ref(),
            Expr::Constant(ast::ExprConstant {
                value: Constant::Ellipsis,
                ..
            })
        ) {
            return true;
        }
    }
    false
}

/// Checks the statement body for unnecessary ellipses.
fn process_body(checker: &mut Checker, parent: &Stmt, body: &[Stmt], expr: &Expr) {
    let has_docstring = starts_with_docstring(body);
    for (element_idx, element) in body.iter().enumerate() {
        if (has_docstring && element_idx == 1 && is_ellipsis(element))
            || (is_ellipsis(element) && body.len() > 1 && element.range() == expr.range())
        {
            let mut diagnostic = Diagnostic::new(UnnecessaryEllipsis, element.range());
            diagnostic.try_set_fix(|| {
                let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
                let edit = delete_stmt(
                    element,
                    Some(parent),
                    &deleted,
                    checker.locator,
                    checker.indexer,
                    checker.stylist,
                )?;

                // In the unlikely event the body consists solely of several
                // ellipses, `delete_stmt` can actually result in a `pass`.
                if edit.is_deletion() && edit.content() == Some("pass") {
                    checker.deletions.insert(RefEquality(element));
                }

                Ok(Fix::automatic(edit))
            });

            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PLW2301
/// Check if the ellipsis constant is used unnecessarily.
/// Emit a warning when:
///    - A line consisting of an ellipsis is preceded by a docstring.
///    - A statement exists in the same scope as the ellipsis.
///      For example: A function consisting of an ellipsis followed by a
///      return statement on the next line.
pub(crate) fn unnecessary_ellipsis(checker: &mut Checker, expr: &Expr) {
    if let Some(stmt) = checker.semantic_model().stmt_parent() {
        if let Stmt::FunctionDef(ast::StmtFunctionDef { body, .. })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { body, .. })
        | Stmt::ClassDef(ast::StmtClassDef { body, .. })
        | Stmt::AsyncFor(ast::StmtAsyncFor { body, .. })
        | Stmt::While(ast::StmtWhile { body, .. })
        | Stmt::With(ast::StmtWith { body, .. })
        | Stmt::AsyncWith(ast::StmtAsyncWith { body, .. })
        | Stmt::If(ast::StmtIf { body, .. })
        | Stmt::Try(ast::StmtTry { body, .. })
        | Stmt::TryStar(ast::StmtTryStar { body, .. }) = stmt
        {
            process_body(checker, stmt, body, expr);
        }
        if let Stmt::If(ast::StmtIf { orelse, .. }) = stmt {
            process_body(checker, stmt, orelse, expr);
        }

        if let Stmt::Match(ast::StmtMatch { cases, .. }) = stmt {
            for case in cases {
                let ast::MatchCase { body, .. } = case;
                process_body(checker, stmt, body, expr);
            }
        }
        if let Stmt::Try(ast::StmtTry {
            handlers,
            orelse,
            finalbody,
            ..
        })
        | Stmt::TryStar(ast::StmtTryStar {
            handlers,
            orelse,
            finalbody,
            ..
        }) = stmt
        {
            for handler in handlers {
                let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { body, .. }) =
                    handler;
                process_body(checker, stmt, body, expr);
            }
            process_body(checker, stmt, orelse, expr);
            process_body(checker, stmt, finalbody, expr);
        }
    }
}
