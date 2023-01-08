use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B016
pub fn cannot_raise_literal(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    let ExprKind::Constant { .. } = &expr.node else {
        return;
    };
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::CannotRaiseLiteral,
        Range::from_located(expr),
    ));
}
