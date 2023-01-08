use rustpython_ast::{Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B002
pub fn unary_prefix_increment(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::UAdd) {
        return;
    }
    let ExprKind::UnaryOp { op, .. } = &operand.node else {
            return;
        };
    if !matches!(op, Unaryop::UAdd) {
        return;
    }
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::UnaryPrefixIncrement,
        Range::from_located(expr),
    ));
}
