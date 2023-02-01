use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S102
pub fn exec_used(expr: &Expr, func: &Expr) -> Option<Diagnostic> {
    let ExprKind::Name { id, .. } = &func.node else {
        return None;
    };
    if id != "exec" {
        return None;
    }
    Some(Diagnostic::new(
        violations::ExecUsed,
        Range::from_located(expr),
    ))
}
