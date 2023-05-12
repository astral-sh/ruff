use log::error;
use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_none, ReturnStatementVisitor};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::types::RefEquality;

use crate::autofix::actions::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
#[violation]
pub struct UselessReturn;

impl AlwaysAutofixableViolation for UselessReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Useless `return` statement at end of function")
    }

    fn autofix_title(&self) -> String {
        format!("Remove useless `return` statement")
    }
}

/// PLR1711
pub(crate) fn useless_return<'a>(
    checker: &mut Checker<'a>,
    stmt: &'a Stmt,
    body: &'a [Stmt],
    returns: Option<&'a Expr>,
) {
    // Skip functions that have a return annotation that is not `None`.
    if !returns.map_or(true, is_const_none) {
        return;
    }

    // Skip empty functions.
    if body.is_empty() {
        return;
    }

    // Find the last statement in the function.
    let last_stmt = body.last().unwrap();
    if !matches!(last_stmt.node, StmtKind::Return(_)) {
        return;
    }

    // Skip functions that consist of a single return statement.
    if body.len() == 1 {
        return;
    }

    // Skip functions that consist of a docstring and a return statement.
    if body.len() == 2 {
        if let StmtKind::Expr(ast::StmtExpr { value }) = &body[0].node {
            if matches!(
                value.node,
                ExprKind::Constant(ast::ExprConstant {
                    value: Constant::Str(_),
                    ..
                })
            ) {
                return;
            }
        }
    }

    // Verify that the last statement is a return statement.
    let StmtKind::Return(ast::StmtReturn { value}) = &last_stmt.node else {
        return;
    };

    // Verify that the return statement is either bare or returns `None`.
    if !value.as_ref().map_or(true, |expr| is_const_none(expr)) {
        return;
    };

    // Finally: verify that there are no _other_ return statements in the function.
    let mut visitor = ReturnStatementVisitor::default();
    visitor.visit_body(body);
    if visitor.returns.len() > 1 {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessReturn, last_stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
        match delete_stmt(
            last_stmt,
            Some(stmt),
            &deleted,
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(edit) => {
                if edit.is_deletion() || edit.content() == Some("pass") {
                    checker.deletions.insert(RefEquality(last_stmt));
                }
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(edit));
            }
            Err(e) => {
                error!("Failed to delete `return` statement: {}", e);
            }
        };
    }
    checker.diagnostics.push(diagnostic);
}
