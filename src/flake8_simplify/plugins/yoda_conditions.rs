use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// SIM300
pub fn yoda_conditions(
    xxxxxxxx: &mut xxxxxxxx,
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

    // Slice exact content to preserve formatting.
    let left_content = xxxxxxxx
        .locator
        .slice_source_code_range(&Range::from_located(left));
    let right_content = xxxxxxxx
        .locator
        .slice_source_code_range(&Range::from_located(right));

    let mut check = Diagnostic::new(
        violations::YodaConditions(left_content.to_string(), right_content.to_string()),
        Range::from_located(expr),
    );

    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            format!("{right_content} == {left_content}"),
            left.location,
            right.end_location.unwrap(),
        ));
    }

    xxxxxxxx.diagnostics.push(check);
}
