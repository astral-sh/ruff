use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// S102
pub fn exec_used(expr: &Expr, func: &Expr) -> Option<Check> {
    let ExprKind::Name { id, .. } = &func.node else {
        return None;
    };
    if id != "exec" {
        return None;
    }
    Some(Check::new(CheckKind::ExecUsed, Range::from_located(expr)))
}
