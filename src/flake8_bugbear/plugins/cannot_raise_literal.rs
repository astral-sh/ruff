use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Check;
use crate::violations;

/// B016
pub fn cannot_raise_literal(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Constant { .. } = &expr.node else {
        return;
    };
    checker.checks.push(Check::new(
        violations::CannotRaiseLiteral,
        Range::from_located(expr),
    ));
}
