use crate::define_simple_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;

define_simple_violation!(ExecUsed, "Use of `exec` detected");

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
