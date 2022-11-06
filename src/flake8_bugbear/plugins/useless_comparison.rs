use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{CheckLocator, Range};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Compare { left, .. } = &expr.node {
        checker.add_check(Check::new(
            CheckKind::UselessComparison,
            checker.locate_check(Range::from_located(left)),
        ));
    }
}
