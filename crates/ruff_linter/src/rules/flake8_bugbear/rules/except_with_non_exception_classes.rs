use std::collections::VecDeque;

use ruff_python_ast::{self as ast, ExceptHandler, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct ExceptWithNonExceptionClasses;

impl Violation for ExceptWithNonExceptionClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`except` handlers should only be exception classes or tuples of exception classes")
    }
}

/// Given an [`Expr`], flatten any [`Expr::Starred`] expressions.
/// This should leave any unstarred iterables alone (subsequently raising a
/// warning for B029).
fn flatten_starred_iterables(expr: &Expr) -> Vec<&Expr> {
    // let Expr::Tuple(ast::ExprTuple { elts, .. }) = expr else {
    //     return vec![expr];
    // };
    // let mut exprs_to_process: VecDeque<&Expr> = elts.iter().collect();
    let mut exprs_to_process: VecDeque<&Expr> = match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().collect(),
        Expr::BinOp(ast::ExprBinOp { .. }) => {
            // vec![left.as_ref(), right.as_ref()].into()
            vec![expr].into()
        }
        _ => return vec![expr],
    };
    let mut flattened_exprs: Vec<&Expr> = Vec::new();
    while let Some(expr) = exprs_to_process.pop_front() {
        match expr {
            Expr::Starred(ast::ExprStarred { value, .. }) => match value.as_ref() {
                Expr::Tuple(ast::ExprTuple { elts, .. })
                | Expr::List(ast::ExprList { elts, .. }) => {
                    exprs_to_process.append(&mut elts.iter().collect());
                }
                _ => flattened_exprs.push(value),
            },
            Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
                // if left or right are tuples, we should flatten them
                match left.as_ref() {
                    Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        exprs_to_process.append(&mut elts.iter().collect());
                    }
                    Expr::Starred(ast::ExprStarred { value, .. }) => {
                        exprs_to_process.push_back(value);
                    }
                    _ => flattened_exprs.push(left),
                }
                match right.as_ref() {
                    Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        exprs_to_process.append(&mut elts.iter().collect());
                    }
                    Expr::Starred(ast::ExprStarred { value, .. }) => {
                        exprs_to_process.push_back(value);
                    }
                    _ => flattened_exprs.push(right),
                }
            }
            _ => flattened_exprs.push(expr),
        }
    }
    flattened_exprs
}

/// Given an [`Expr`], flatten any binary operations.
fn flatten_bin_op(expr: &Expr) -> Vec<&Expr> {
    let mut stack = vec![expr];
    let mut flattened_exprs = Vec::new();

    while let Some(expr) = stack.pop() {
        if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = expr {
            stack.push(left);
            stack.push(right);
        // } else if let Expr::Tuple(ast::ExprTuple { elts, .. }) = expr {
        //     stack.extend(elts.iter());
        } else {
            println!("Pushing expression: {:#?}", expr);
            flattened_exprs.push(expr);
        }
    }

    flattened_exprs
}

/// B030
pub(crate) fn except_with_non_exception_classes(
    checker: &mut Checker,
    except_handler: &ExceptHandler,
) {
    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) =
        except_handler;
    let Some(type_) = type_ else {
        return;
    };
    // let flattened_exprs = flatten_starred_iterables(type_);
    // println!("{:#?}", flattened_exprs);
    // let flattened_exprs = flattened_exprs.iter().flat_map(|expr| flatten_bin_op(expr));
    for expr in flatten_starred_iterables(type_) {
        if !matches!(
            expr,
            Expr::Subscript(_) | Expr::Attribute(_) | Expr::Name(_) | Expr::Call(_),
        ) {
            println!("Adding diagnostic for expression: {:#?}", expr);
            checker
                .diagnostics
                .push(Diagnostic::new(ExceptWithNonExceptionClasses, expr.range()));
        }
    }
}
