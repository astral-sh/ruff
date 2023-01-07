use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};

pub fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if matches!(expr.node, ExprKind::Compare { .. }) {
        checker.checks.push(Check::new(
            CheckKind::UselessComparison,
            Range::from_located(expr),
        ));
    }
}
