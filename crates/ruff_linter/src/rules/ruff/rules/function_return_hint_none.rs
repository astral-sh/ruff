use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_none, ReturnStatementVisitor};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Expr, Stmt};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::super::branch::Branch;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Examples
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct FunctionReturnHintNone;
// TODO naming convention

impl AlwaysFixableViolation for FunctionReturnHintNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary key check before dictionary access")
    }

    fn fix_title(&self) -> String {
        format!("Replace with `dict.get`")
    }
}

/// RUF020
pub(crate) fn function_return_hint_none(
    checker: &mut Checker,
    body: &[Stmt],
    returns: Option<&Expr>,
) {
    // Find the last statement in the function.
    let Some(last_stmt) = body.last() else {
        // Skip empty functions.
        return;
    };

    // Skip functions that consist of a single return statement.
    if body.len() == 1 && matches!(last_stmt, Stmt::Return(_)) {
        return;
    }

    // Traverse the function body, to collect the stack.
    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    // Avoid false positives for generators.
    // if stack.is_generator {
    //     return;
    // }

    // Skip any functions without return statements.
    if stack.returns.is_empty() {
        return;
    }

    // If we have at least one non-`None` return...
    if result_exists(&stack.returns) {
        if checker.enabled(Rule::ImplicitReturnValue) {
            implicit_return_value(checker, &stack);
        }
        if checker.enabled(Rule::ImplicitReturn) {
            implicit_return(checker, last_stmt);
        }

        if checker.enabled(Rule::UnnecessaryAssign) {
            unnecessary_assign(checker, &stack);
        }
    } else {
        if checker.enabled(Rule::UnnecessaryReturnNone) {
            // Skip functions that have a return annotation that is not `None`.
            if returns.map_or(true, is_const_none) {
                unnecessary_return_none(checker, &stack);
            }
        }
    }
}
