use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};
use crate::violations;

/// S102
pub fn exec_used(expr: &Expr, func: &Expr) -> Option<Check> {
    let ExprKind::Name { id, .. } = &func.node else {
        return None;
    };
    if id != "exec" {
        return None;
    }
    Some(Check::new(violations::ExecUsed, Range::from_located(expr)))
}
