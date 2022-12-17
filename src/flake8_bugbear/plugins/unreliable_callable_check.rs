use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// B004
pub fn unreliable_callable_check(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
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
    checker.add_check(Check::new(
        CheckKind::UnreliableCallableCheck,
        Range::from_located(expr),
    ));
}
