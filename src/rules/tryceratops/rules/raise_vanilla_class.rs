use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RaiseVanillaClass;
);
impl Violation for RaiseVanillaClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Create your own exception")
    }
}

fn get_name_expr(expr: &Expr) -> Option<&Expr> {
    match &expr.node {
        ExprKind::Call { func, .. } => get_name_expr(func),
        ExprKind::Name { .. } => Some(expr),
        _ => None,
    }
}

/// TRY002
pub fn raise_vanilla_class(checker: &mut Checker, expr: &Expr) {
    if let Some(func) = get_name_expr(expr) {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "Exception" {
                checker.diagnostics.push(Diagnostic::new(
                    RaiseVanillaClass,
                    Range::from_located(expr),
                ));
            }
        }
    }
}
