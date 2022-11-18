use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn match_not_implemented(expr: &Expr) -> Option<&Expr> {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { id, .. } = &func.node {
                if id == "NotImplemented" {
                    return Some(func);
                }
            }
        }
        ExprKind::Name { id, .. } => {
            if id == "NotImplemented" {
                return Some(expr);
            }
        }
        _ => {}
    }
    None
}

/// F901
pub fn raise_not_implemented(checker: &mut Checker, expr: &Expr) {
    if let Some(expr) = match_not_implemented(expr) {
        let mut check = Check::new(CheckKind::RaiseNotImplemented, Range::from_located(expr));
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                "NotImplementedError".to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ))
        }
        checker.add_check(check);
    }
}
