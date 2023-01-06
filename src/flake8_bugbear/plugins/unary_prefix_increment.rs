use rustpython_ast::{Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};
use crate::violations;

/// B002
pub fn unary_prefix_increment(checker: &mut Checker, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::UAdd) {
        return;
    }
    let ExprKind::UnaryOp { op, .. } = &operand.node else {
            return;
        };
    if !matches!(op, Unaryop::UAdd) {
        return;
    }
    checker.checks.push(Check::new(
        violations::UnaryPrefixIncrement,
        Range::from_located(expr),
    ));
}
