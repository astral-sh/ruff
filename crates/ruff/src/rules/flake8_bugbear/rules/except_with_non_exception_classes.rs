use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Located};

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

#[violation]
pub struct ExceptWithNonExceptionClasses;

impl Violation for ExceptWithNonExceptionClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Except handlers should only be exception classes or tuples of exception classes")
    }
}

/// Given a set of Expr, finds any that are starred and flattens them.
/// This should leave any unstarred iterables alone (subsequently raising a
/// warning for B029).
fn flatten_starred_iterables(exprs: &[Expr]) -> Vec<&Expr> {
    let mut flattened_exprs: Vec<&Expr> = Vec::new();
    let mut expr_lists_to_flatten = vec![exprs];
    while !expr_lists_to_flatten.is_empty() {
        let expr_list = expr_lists_to_flatten.pop().unwrap();
        for expr in expr_list {
            match &expr.node {
                ExprKind::Starred { value, .. } => match &value.as_ref().node {
                    ExprKind::Tuple { elts, .. } | ExprKind::List { elts, .. } => {
                        expr_lists_to_flatten.push(elts.as_slice());
                    }
                    _ => flattened_exprs.push(expr),
                },
                _ => flattened_exprs.push(expr),
            }
        }
    }

    flattened_exprs
}

/// B030
pub fn except_with_non_exception_classes(checker: &mut Checker, excepthandler: &Excepthandler) {
    let ExcepthandlerKind::ExceptHandler { type_, .. } = &excepthandler.node;
    if type_.is_none() {
        return;
    }
    let exprs: Vec<&Expr> = match type_.as_ref().unwrap().as_ref() {
        Located {
            node: ExprKind::Tuple { elts, .. },
            ..
        } => flatten_starred_iterables(elts),
        any => vec![any],
    };

    for expr in exprs {
        match expr.node {
            ExprKind::Attribute { .. } | ExprKind::Name { .. } | ExprKind::Call { .. } => (),
            _ => checker.diagnostics.push(Diagnostic::new(
                ExceptWithNonExceptionClasses,
                Range::from_located(expr),
            )),
        }
    }
}
