use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B004
pub fn unreliable_callable_check(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "getattr" || id == "hasattr" {
            if args.len() >= 2 {
                if let ExprKind::Constant {
                    value: Constant::Str(s),
                    ..
                } = &args[1].node
                {
                    if s == "__call__" {
                        checker.add_check(Check::new(
                            CheckKind::UnreliableCallableCheck,
                            Range::from_located(expr),
                        ));
                    }
                }
            }
        }
    }
}
