use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

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

    let check = Check::new(
        CheckKind::YodaConditions(left.to_string(), right.to_string()),
        Range::from_located(expr),
    );

    checker.add_check(check);
}
