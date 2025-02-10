use ruff_python_ast::Expr;
use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Modules;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usage of `datetime.date.fromtimestamp()`.
///
/// ## Why is this bad?
/// Python datetime objects can be naive or timezone-aware. While an aware
/// object represents a specific moment in time, a naive object does not
/// contain enough information to unambiguously locate itself relative to other
/// datetime objects. Since this can lead to errors, it is recommended to
/// always use timezone-aware objects.
///
/// `datetime.date.fromtimestamp(ts)` returns a naive datetime object.
/// Instead, use `datetime.datetime.fromtimestamp(ts, tz=...)` to create a
/// timezone-aware object.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.date.fromtimestamp(946684800)
/// ```
///
/// Use instead:
/// ```python
/// import datetime
///
/// datetime.datetime.fromtimestamp(946684800, tz=datetime.timezone.utc)
/// ```
///
/// Or, for Python 3.11 and later:
/// ```python
/// import datetime
///
/// datetime.datetime.fromtimestamp(946684800, tz=datetime.UTC)
/// ```
///
/// ## References
/// - [Python documentation: Aware and Naive Objects](https://docs.python.org/3/library/datetime.html#aware-and-naive-objects)
#[derive(ViolationMetadata)]
pub(crate) struct CallDateFromtimestamp;

impl Violation for CallDateFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`datetime.date.fromtimestamp()` used".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `datetime.datetime.fromtimestamp(ts, tz=...).date()` instead".to_string())
    }
}

pub(crate) fn call_date_fromtimestamp(checker: &Checker, func: &Expr, location: TextRange) {
    if !checker.semantic().seen_module(Modules::DATETIME) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["datetime", "date", "fromtimestamp"]
            )
        })
    {
        checker.report_diagnostic(Diagnostic::new(CallDateFromtimestamp, location));
    }
}
