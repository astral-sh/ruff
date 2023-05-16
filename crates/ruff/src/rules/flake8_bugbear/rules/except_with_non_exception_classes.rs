use std::collections::VecDeque;

use rustpython_parser::ast::{self, Excepthandler, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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
    let Expr::Tuple(ast::ExprTuple { elts, .. } )= expr else {
        return vec![expr];
    };
    let mut flattened_exprs: Vec<&Expr> = Vec::with_capacity(elts.len());
    let mut exprs_to_process: VecDeque<&Expr> = elts.iter().collect();
    while let Some(expr) = exprs_to_process.pop_front() {
        match expr {
            Expr::Starred(ast::ExprStarred { value, .. }) => match value.as_ref() {
                Expr::Tuple(ast::ExprTuple { elts, .. })
                | Expr::List(ast::ExprList { elts, .. }) => {
                    exprs_to_process.append(&mut elts.iter().collect());
                }
                _ => flattened_exprs.push(value),
            },
            _ => flattened_exprs.push(expr),
        }
    }
    flattened_exprs
}

/// B030
pub(crate) fn except_with_non_exception_classes(
    checker: &mut Checker,
    excepthandler: &Excepthandler,
) {
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = excepthandler;
    let Some(type_) = type_ else {
        return;
    };
    for expr in flatten_starred_iterables(type_) {
        if !matches!(
            expr,
            Expr::Subscript(_) | Expr::Attribute(_) | Expr::Name(_) | Expr::Call(_),
        ) {
            checker
                .diagnostics
                .push(Diagnostic::new(ExceptWithNonExceptionClasses, expr.range()));
        }
    }
}
