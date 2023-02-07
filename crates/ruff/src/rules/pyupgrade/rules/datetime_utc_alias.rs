use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::helpers::collect_call_path;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{Availability, Violation};
use crate::AutofixKind;

define_violation!(
    pub struct DatetimeTimezoneUTC {
        pub straight_import: bool,
    }
);
impl Violation for DatetimeTimezoneUTC {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `datetime.UTC` alias")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        if self.straight_import {
            Some(|_| "Convert to `datetime.UTC` alias".to_string())
        } else {
            None
        }
    }
}

/// UP017
pub fn datetime_utc_alias(checker: &mut Checker, expr: &Expr) {
    if checker.resolve_call_path(expr).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "timezone", "utc"]
    }) {
        let straight_import = collect_call_path(expr).as_slice() == ["datetime", "timezone", "utc"];
        let mut diagnostic = Diagnostic::new(
            DatetimeTimezoneUTC { straight_import },
            Range::from_located(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
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
