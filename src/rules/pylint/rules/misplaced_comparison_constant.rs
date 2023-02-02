use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;

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
    let mut diagnostic = Diagnostic::new(
        MisplacedComparisonConstant(suggestion.clone()),
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            suggestion,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
