use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{CheckLocator, Range};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B016
pub fn cannot_raise_literal(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Constant { .. } = &expr.node {
        checker.add_check(Check::new(
            CheckKind::CannotRaiseLiteral,
            checker.locate_check(Range::from_located(expr)),
        ));
    }
}
