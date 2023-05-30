use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
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
    if let Some(first_stmt) = body.first() {
        if let Stmt::Expr(ast::StmtExpr { value, .. }) = first_stmt {
            if matches!(
                value.as_ref(),
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(_),
                    ..
                })
            ) {
                return true;
            }
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

/// PLW2301
/// Check if the ellipsis constant is used unnecessarily.
/// Emit a warning when:
///    - A line consisting of an ellipsis is preceded by a docstring.
///    - A statement exists in the same scope as the ellipsis.
///      For example: A function consisting of an ellipsis followed by a
///      return statement on the next line.
pub(crate) fn unnecessary_ellipsis(checker: &mut Checker, body: &[Stmt]) {
    let has_docstring = starts_with_docstring(body);
    for (element_idx, element) in body.iter().enumerate() {
        if (has_docstring && element_idx == 1 && is_ellipsis(element))
            || (is_ellipsis(element) && body.len() > 1)
        {
            let mut diagnostic = Diagnostic::new(UnnecessaryEllipsis, element.range());
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::deletion(
                element.start(),
                element.end(),
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}
