use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

pub fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if matches!(expr.node, ExprKind::Compare { .. }) {
        checker.diagnostics.push(Diagnostic::new(
            violations::UselessComparison,
            Range::from_located(expr),
        ));
    }
}
