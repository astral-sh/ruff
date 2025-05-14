use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::ReturnStatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for functions that end with an unnecessary `return` or
/// `return None`, and contain no other `return` statements.
///
/// ## Why is this bad?
/// Python implicitly assumes a `None` return at the end of a function, making
/// it unnecessary to explicitly write `return None`.
///
/// ## Example
/// ```python
/// def f():
///     print(5)
///     return None
/// ```
///
/// Use instead:
/// ```python
/// def f():
///     print(5)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UselessReturn;

impl AlwaysFixableViolation for UselessReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Useless `return` statement at end of function".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove useless `return` statement".to_string()
    }
}

/// PLR1711
pub(crate) fn useless_return(
    checker: &Checker,
    stmt: &Stmt,
    body: &[Stmt],
    returns: Option<&Expr>,
) {
    // Skip functions that have a return annotation that is not `None`.
    if !returns.is_none_or(Expr::is_none_literal_expr) {
        return;
    }

    // Find the last statement in the function.
    let Some(last_stmt) = body.last() else {
        // Skip empty functions.
        return;
    };

    // Verify that the last statement is a return statement.
    let Stmt::Return(ast::StmtReturn { value, range: _ }) = &last_stmt else {
        return;
    };

    // Skip functions that consist of a single return statement.
    if body.len() == 1 {
        return;
    }

    // Skip functions that consist of a docstring and a return statement.
    if body.len() == 2 {
        if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = &body[0] {
            if value.is_string_literal_expr() {
                return;
            }
        }
    }

    // Verify that the return statement is either bare or returns `None`.
    if !value
        .as_ref()
        .is_none_or(|expr| expr.is_none_literal_expr())
    {
        return;
    }

    // Finally: verify that there are no _other_ return statements in the function.
    let mut visitor = ReturnStatementVisitor::default();
    visitor.visit_body(body);
    if visitor.returns.len() > 1 {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessReturn, last_stmt.range());
    let edit = fix::edits::delete_stmt(last_stmt, Some(stmt), checker.locator(), checker.indexer());
    diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
        checker.semantic().current_statement_id(),
    )));
    checker.report_diagnostic(diagnostic);
}
