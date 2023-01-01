use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// SIM300
pub fn yoda_conditions(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    if !matches!(ops[..], [Cmpop::Eq]) {
        return;
    }

    if comparators.len() != 1 {
        return;
    }

    if !matches!(left.node, ExprKind::Constant { .. }) {
        return;
    }

    let right = comparators.first().unwrap();
    if matches!(left.node, ExprKind::Constant { .. })
        & matches!(right.node, ExprKind::Constant { .. })
    {
        return;
    }

    let check = Check::new(
        CheckKind::YodaConditions(left.to_string(), right.to_string()),
        Range::from_located(expr),
    );

    checker.add_check(check);
}
