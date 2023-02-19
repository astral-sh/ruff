use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{
    Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind,
};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct IgnoringException;
);
impl Violation for IgnoringException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider at least logging the exception")
    }
}

fn is_vanilla_exception(expr: &Expr) -> bool {
    if let ExprKind::Name { id, .. } = &expr.node {
        if id == "Exception" {
            return true;
        }
    }
    false
}

fn wraps_vanilla_exception(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Name { .. } => is_vanilla_exception(expr),
        ExprKind::Tuple { elts, .. } => elts.iter().any(is_vanilla_exception),
        _ => false,
    }
}

fn is_skipper(stmt: &Stmt) -> bool {
    match &stmt.node {
        StmtKind::Pass => true,
        StmtKind::Expr { value } => match &value.node {
            ExprKind::Constant {
                value: Constant::Ellipsis,
                ..
            } => true,
            ExprKind::Name { id, .. } => id == "Ellipsis",
            _ => false,
        },
        _ => false,
    }
}

/// TRY202
pub fn ignoring_exception(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { body, type_, .. } = &handler.node;
        if let Some(clean_body) = body.get(0) {
            if let Some(clean_type) = type_ {
                if wraps_vanilla_exception(clean_type) && is_skipper(clean_body) {
                    checker.diagnostics.push(Diagnostic::new(
                        IgnoringException,
                        Range::from_located(clean_body),
                    ));
                }
            }
        }
    }
}
