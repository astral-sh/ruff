use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DatetimeTimezoneUTC {
    pub straight_import: bool,
}

impl Violation for DatetimeTimezoneUTC {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

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
    if checker
        .ctx
        .resolve_call_path(expr)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["datetime", "timezone", "utc"]
        })
    {
        let straight_import = collect_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["datetime", "timezone", "utc"]
        });
        let mut diagnostic =
            Diagnostic::new(DatetimeTimezoneUTC { straight_import }, Range::from(expr));
        if checker.patch(diagnostic.kind.rule()) {
            if straight_import {
                diagnostic.set_fix(Edit::replacement(
                    "datetime.UTC".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
