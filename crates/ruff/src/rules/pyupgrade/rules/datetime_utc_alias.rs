use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct DatetimeTimezoneUTC;

impl Violation for DatetimeTimezoneUTC {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `datetime.UTC` alias")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Convert to `datetime.UTC` alias".to_string())
    }
}

/// UP017
pub(crate) fn datetime_utc_alias(checker: &mut Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(expr)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "timezone", "utc"])
        })
    {
        let mut diagnostic = Diagnostic::new(DatetimeTimezoneUTC, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            // If the reference was structured as, e.g., `datetime.timezone.utc`, then we can
            // replace it with `datetime.UTC`. If `timezone` was imported via `from datetime import
            // timezone`, then the replacement is more complicated.
            if collect_call_path(expr).map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["datetime", "timezone", "utc"])
            }) {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    "datetime.UTC".to_string(),
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
