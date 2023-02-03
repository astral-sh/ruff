use crate::define_simple_autofix_violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_simple_autofix_violation!(
    RaiseNotImplemented,
    "`raise NotImplemented` should be `raise NotImplementedError`",
    "Use `raise NotImplementedError`"
);

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
    let Some(expr) = match_not_implemented(expr) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(RaiseNotImplemented, Range::from_located(expr));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            "NotImplementedError".to_string(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
