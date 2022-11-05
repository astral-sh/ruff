use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Compare { left, .. } = &expr.node {
        checker.add_check(Check::new(
            CheckKind::UselessComparison,
            Range::from_located(left),
        ));
    }
}
