use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::python::string::{self};
use crate::registry::Diagnostic;
use crate::violations;

/// SIM300
pub fn yoda_conditions(
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

    // Slice exact content to preserve formatting.
    let constant = checker
        .locator
        .slice_source_code_range(&Range::from_located(left));
    let variable = checker
        .locator
        .slice_source_code_range(&Range::from_located(right));

    if !matches!(&left.node, &ExprKind::Constant { .. }) && !string::is_upper(constant) {
        return;
    }
    if matches!(&right.node, &ExprKind::Constant { .. }) || string::is_upper(variable) {
        return;
    }

    // Reverse the operation.
    let reversed_op = match op {
        Cmpop::Eq => "==",
        Cmpop::NotEq => "!=",
        Cmpop::Lt => ">",
        Cmpop::LtE => ">=",
        Cmpop::Gt => "<",
        Cmpop::GtE => "<=",
        _ => unreachable!("Expected comparison operator"),
    };

    let suggestion = format!("{variable} {reversed_op} {constant}");
    let mut diagnostic = Diagnostic::new(
        violations::YodaConditions {
            suggestion: suggestion.to_string(),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            suggestion,
            left.location,
            right.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
