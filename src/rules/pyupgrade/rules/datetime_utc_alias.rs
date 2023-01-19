use rustpython_ast::Expr;

use crate::ast::helpers::collect_call_path;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// UP017
pub fn datetime_utc_alias(checker: &mut Checker, expr: &Expr) {
    if checker.resolve_call_path(expr).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "timezone", "utc"]
    }) {
        let straight_import = collect_call_path(expr).as_slice() == ["datetime", "timezone", "utc"];
        let mut diagnostic = Diagnostic::new(
            violations::DatetimeTimezoneUTC { straight_import },
            Range::from_located(expr),
        );
        if checker.patch(&Rule::DatetimeTimezoneUTC) {
            if straight_import {
                diagnostic.amend(Fix::replacement(
                    "datetime.UTC".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
