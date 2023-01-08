use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, compose_call_path, dealias_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP017
pub fn datetime_utc_alias(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    let dealiased_call_path = dealias_call_path(collect_call_paths(expr), &xxxxxxxx.import_aliases);
    if dealiased_call_path == ["datetime", "timezone", "utc"] {
        let mut check = Diagnostic::new(violations::DatetimeTimezoneUTC, Range::from_located(expr));
        if xxxxxxxx.patch(&RuleCode::UP017) {
            check.amend(Fix::replacement(
                compose_call_path(expr)
                    .unwrap()
                    .replace("timezone.utc", "UTC"),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        xxxxxxxx.diagnostics.push(check);
    }
}
