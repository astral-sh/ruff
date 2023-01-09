use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, compose_call_path, dealias_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;

/// UP017
pub fn datetime_utc_alias(checker: &mut Checker, expr: &Expr) {
    let dealiased_call_path = dealias_call_path(collect_call_paths(expr), &checker.import_aliases);
    if dealiased_call_path == ["datetime", "timezone", "utc"] {
        let mut diagnostic =
            Diagnostic::new(violations::DatetimeTimezoneUTC, Range::from_located(expr));
        if checker.patch(&RuleCode::UP017) {
            diagnostic.amend(Fix::replacement(
                compose_call_path(expr)
                    .unwrap()
                    .replace("timezone.utc", "UTC"),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
