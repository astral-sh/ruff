use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UnreliableCallableCheck;
);
impl Violation for UnreliableCallableCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            " Using `hasattr(x, '__call__')` to test if x is callable is unreliable. Use \
             `callable(x)` for consistent results."
        )
    }
}

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
