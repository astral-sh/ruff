use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::{violations, Diagnostic};

/// PLC2201
pub fn misplaced_comparison_constant(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    if !matches!(
        op,
        Cmpop::Eq | Cmpop::NotEq | Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE,
    ) {
        return;
    }
    if !matches!(&left.node, &ExprKind::Constant { .. }) {
        return;
    }
    if matches!(&right.node, &ExprKind::Constant { .. }) {
        return;
    }

    let reversed_op = match op {
        Cmpop::Eq => "==",
        Cmpop::NotEq => "!=",
        Cmpop::Lt => ">",
        Cmpop::LtE => ">=",
        Cmpop::Gt => "<",
        Cmpop::GtE => "<=",
        _ => unreachable!("Expected comparison operator"),
    };
    let suggestion = format!("{right} {reversed_op} {left}");
    let mut check = Diagnostic::new(
        violations::MisplacedComparisonConstant(suggestion.clone()),
        Range::from_located(expr),
    );
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            suggestion,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.checks.push(check);
}
