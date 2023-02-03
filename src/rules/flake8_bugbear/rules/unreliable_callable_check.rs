use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Constant, Expr, ExprKind};

define_simple_violation!(
    UnreliableCallableCheck,
    " Using `hasattr(x, '__call__')` to test if x is callable is unreliable. Use \
             `callable(x)` for consistent results."
);

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
    checker.diagnostics.push(Diagnostic::new(
        UnreliableCallableCheck,
        Range::from_located(expr),
    ));
}
