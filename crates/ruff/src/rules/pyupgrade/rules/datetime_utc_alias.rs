use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `datetime.timezone.utc`.
///
/// ## Why is this bad?
/// As of Python 3.11, `datetime.UTC` is an alias for `datetime.timezone.utc`.
/// This alias is less verbose and more readable.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.timezone.utc
/// ```
///
/// Use instead:
/// ```python
/// import datetime
///
/// datetime.UTC
/// ```
///
/// ## References
/// - [Python documentation: `datetime.UTC`](https://docs.python.org/3/library/datetime.html#datetime.UTC)
#[violation]
pub struct DatetimeTimezoneUTC {
    straight_import: bool,
}

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
        .semantic_model()
        .resolve_call_path(expr)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["datetime", "timezone", "utc"]
        })
    {
        let straight_import = collect_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["datetime", "timezone", "utc"]
        });
        let mut diagnostic = Diagnostic::new(DatetimeTimezoneUTC { straight_import }, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            if straight_import {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    "datetime.UTC".to_string(),
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
