use rustpython_ast::{Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B002
pub fn unary_prefix_increment(checker: &mut Checker, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if matches!(op, Unaryop::UAdd) {
        if let ExprKind::UnaryOp { op, .. } = &operand.node {
            if matches!(op, Unaryop::UAdd) {
                checker.add_check(Check::new(
                    CheckKind::UnaryPrefixIncrement,
                    Range::from_located(expr),
                ))
            }
        }
    }
}
