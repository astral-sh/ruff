use std::collections::VecDeque;

use ruff_python_ast::{self as ast, ExceptHandler, Expr, Operator};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for exception handlers that catch non-exception classes.
///
/// ## Why is this bad?
/// Catching classes that do not inherit from `BaseException` will raise a
/// `TypeError`.
///
/// ## Example
/// ```python
/// try:
///     1 / 0
/// except 1:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     1 / 0
/// except ZeroDivisionError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
/// - [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
#[derive(ViolationMetadata)]
pub(crate) struct ExceptWithNonExceptionClasses {
    is_star: bool,
}

impl Violation for ExceptWithNonExceptionClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_star {
            "`except*` handlers should only be exception classes or tuples of exception classes"
                .to_string()
        } else {
            "`except` handlers should only be exception classes or tuples of exception classes"
                .to_string()
        }
    }
}

/// B030
pub(crate) fn except_with_non_exception_classes(checker: &Checker, except_handler: &ExceptHandler) {
    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) =
        except_handler;
    let Some(type_) = type_ else {
        return;
    };
    for expr in flatten_iterables(type_) {
        if !matches!(
            expr,
            Expr::Subscript(_) | Expr::Attribute(_) | Expr::Name(_) | Expr::Call(_),
        ) {
            let is_star = checker
                .semantic()
                .current_statement()
                .as_try_stmt()
                .is_some_and(|try_stmt| try_stmt.is_star);
            checker.report_diagnostic(Diagnostic::new(
                ExceptWithNonExceptionClasses { is_star },
                expr.range(),
            ));
        }
    }
}

/// Given an [`Expr`], flatten any [`Expr::Starred`] expressions and any
/// [`Expr::BinOp`] expressions into a flat list of expressions.
///
/// This should leave any unstarred iterables alone (subsequently raising a
/// warning for B029).
fn flatten_iterables(expr: &Expr) -> Vec<&Expr> {
    // Unpack the top-level Tuple into queue, otherwise add as-is.
    let mut exprs_to_process: VecDeque<&Expr> = match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().collect(),
        _ => vec![expr].into(),
    };
    let mut flattened_exprs: Vec<&Expr> = Vec::with_capacity(exprs_to_process.len());

    while let Some(expr) = exprs_to_process.pop_front() {
        match expr {
            Expr::Starred(ast::ExprStarred { value, .. }) => match value.as_ref() {
                Expr::Tuple(ast::ExprTuple { elts, .. })
                | Expr::List(ast::ExprList { elts, .. }) => {
                    exprs_to_process.append(&mut elts.iter().collect());
                }
                Expr::BinOp(ast::ExprBinOp {
                    op: Operator::Add, ..
                }) => {
                    exprs_to_process.push_back(value);
                }
                _ => flattened_exprs.push(value),
            },
            Expr::BinOp(ast::ExprBinOp {
                left,
                right,
                op: Operator::Add,
                ..
            }) => {
                for expr in [left, right] {
                    // If left or right are tuples, starred, or binary operators, flatten them.
                    match expr.as_ref() {
                        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                            exprs_to_process.append(&mut elts.iter().collect());
                        }
                        Expr::Starred(ast::ExprStarred { value, .. }) => {
                            exprs_to_process.push_back(value);
                        }
                        Expr::BinOp(ast::ExprBinOp {
                            op: Operator::Add, ..
                        }) => {
                            exprs_to_process.push_back(expr);
                        }
                        _ => flattened_exprs.push(expr),
                    }
                }
            }
            _ => flattened_exprs.push(expr),
        }
    }

    flattened_exprs
}
