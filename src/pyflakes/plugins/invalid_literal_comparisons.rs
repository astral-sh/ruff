use itertools::izip;
use rustpython_ast::{Cmpop, Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn is_singleton(expr: &Expr) -> bool {
    matches!(
        expr.node,
        ExprKind::Constant {
            value: Constant::None | Constant::Bool(_) | Constant::Ellipsis,
            ..
        }
    )
}

fn is_constant(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant { .. } => true,
        ExprKind::Tuple { elts, .. } => elts.iter().all(is_constant),
        _ => false,
    }
}

fn is_constant_non_singleton(expr: &Expr) -> bool {
    is_constant(expr) && !is_singleton(expr)
}

/// F632
pub fn invalid_literal_comparison(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    location: Range,
) {
    let mut left = left;
    for (op, right) in izip!(ops, comparators) {
        if matches!(op, Cmpop::Is | Cmpop::IsNot)
            && (is_constant_non_singleton(left) || is_constant_non_singleton(right))
        {
            checker.add_check(Check::new(CheckKind::IsLiteral, location));
        }
        left = right;
    }
}
