use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of `datetime.timezone.utc`.
///
/// ## Why is this bad?
/// As of Python 3.11, `datetime.UTC` is an alias for `datetime.timezone.utc`.
/// The alias is more readable and generally preferred over the full path.
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
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `datetime.UTC`](https://docs.python.org/3/library/datetime.html#datetime.UTC)
#[derive(ViolationMetadata)]
pub(crate) struct DatetimeTimezoneUTC;

impl Violation for DatetimeTimezoneUTC {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `datetime.UTC` alias".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to `datetime.UTC` alias".to_string())
    }
}

/// UP017
pub(crate) fn datetime_utc_alias(checker: &Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["datetime", "timezone", "utc"])
        })
    {
        let mut diagnostic = Diagnostic::new(DatetimeTimezoneUTC, expr.range());
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from("datetime", "UTC"),
                expr.start(),
                checker.semantic(),
            )?;
            let reference_edit = Edit::range_replacement(binding, expr.range());
            Ok(Fix::safe_edits(import_edit, [reference_edit]))
        });
        checker.report_diagnostic(diagnostic);
    }
}
