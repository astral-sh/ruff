use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// S102
pub fn exec_used(expr: &Expr, func: &Expr) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "exec" {
            return Some(Check::new(CheckKind::ExecUsed, Range::from_located(expr)));
        }
    }
    None
}
