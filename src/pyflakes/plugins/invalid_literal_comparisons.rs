use itertools::izip;
use log::error;
use rustpython_ast::{Cmpop, Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::pyflakes::fixes::fix_invalid_literal_comparison;

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
            let mut check = Check::new(CheckKind::IsLiteral, location);
            if checker.patch(check.kind.code()) {
                match fix_invalid_literal_comparison(
                    checker.locator,
                    Range {
                        location: left.location,
                        end_location: right.end_location.unwrap(),
                    },
                ) {
                    Ok(fix) => check.amend(fix),
                    Err(e) => error!("Failed to fix invalid comparison: {}", e),
                }
            }
            checker.add_check(check);
        }
        left = right;
    }
}
