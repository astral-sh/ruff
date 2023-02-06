use crate::define_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;

define_violation!(
    pub struct ExecUsed;
);
impl Violation for ExecUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `exec` detected")
    }
}

/// S102
pub fn exec_used(expr: &Expr, func: &Expr) -> Option<Diagnostic> {
    let ExprKind::Name { id, .. } = &func.node else {
        return None;
    };
    if id != "exec" {
        return None;
    }
    Some(Diagnostic::new(ExecUsed, Range::from_located(expr)))
}
