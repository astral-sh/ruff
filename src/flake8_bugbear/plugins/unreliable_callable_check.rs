use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B004
pub fn unreliable_callable_check(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, func: &Expr, args: &[Expr]) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    if id != "getattr" && id != "hasattr" {
        return;
    }
    if args.len() < 2 {
        return;
    };
    let ExprKind::Constant {
        value: Constant::Str(s),
        ..
    } = &args[1].node else
    {
        return;
    };
    if s != "__call__" {
        return;
    }
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::UnreliableCallableCheck,
        Range::from_located(expr),
    ));
}
