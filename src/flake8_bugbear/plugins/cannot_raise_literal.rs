use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// B016
pub fn cannot_raise_literal(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Constant { .. } = &expr.node else {
        return;
    };
    checker.add_check(Check::new(
        CheckKind::CannotRaiseLiteral,
        Range::from_located(expr),
    ));
}
