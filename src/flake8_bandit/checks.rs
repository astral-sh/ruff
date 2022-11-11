use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// S102
pub fn exec_used(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "exec" {
            checker.add_check(Check::new(CheckKind::ExecUsed, Range::from_located(expr)));
        }
    }
}
