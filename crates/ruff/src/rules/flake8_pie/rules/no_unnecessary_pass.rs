use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::helpers::trailing_comment_start_offset;

use crate::autofix::actions::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary `pass` statements in class and function bodies.
/// where it is not needed syntactically (e.g., when an indented docstring is
/// present).
///
/// ## Why is this bad?
/// When a function or class definition contains a docstring, an additional
/// `pass` statement is redundant.
///
/// ## Example
/// ```python
/// def foo():
///     """Placeholder docstring."""
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     """Placeholder docstring."""
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-pass-statement)
#[violation]
pub struct UnnecessaryPass;

impl AlwaysAutofixableViolation for UnnecessaryPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `pass` statement")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }
}

/// PIE790
pub(crate) fn no_unnecessary_pass(checker: &mut Checker, body: &[Stmt]) {
    if body.len() > 1 {
        // This only catches the case in which a docstring makes a `pass` statement
        // redundant. Consider removing all `pass` statements instead.
        let docstring_stmt = &body[0];
        let pass_stmt = &body[1];
        let Stmt::Expr(ast::StmtExpr { value, range: _ } )= docstring_stmt else {
            return;
        };
        if matches!(
            value.as_ref(),
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(..),
                ..
            })
        ) {
            if pass_stmt.is_pass_stmt() {
                let mut diagnostic = Diagnostic::new(UnnecessaryPass, pass_stmt.range());
                if checker.patch(diagnostic.kind.rule()) {
                    if let Some(index) = trailing_comment_start_offset(pass_stmt, checker.locator) {
                        diagnostic.set_fix(Fix::automatic(Edit::range_deletion(
                            pass_stmt.range().add_end(index),
                        )));
                    } else {
                        #[allow(deprecated)]
                        diagnostic.try_set_fix_from_edit(|| {
                            delete_stmt(
                                pass_stmt,
                                None,
                                &[],
                                checker.locator,
                                checker.indexer,
                                checker.stylist,
                            )
                        });
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
