use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

pub fn useless_comparison(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    if matches!(expr.node, ExprKind::Compare { .. }) {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::UselessComparison,
            Range::from_located(expr),
        ));
    }
}
