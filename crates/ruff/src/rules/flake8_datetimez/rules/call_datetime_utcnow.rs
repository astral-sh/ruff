use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct CallDatetimeUtcnow;

impl Violation for CallDatetimeUtcnow {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.utcnow()` is not allowed, use \
             `datetime.datetime.now(tz=)` instead"
        )
    }
}

/// Checks for `datetime.datetime.today()`. (DTZ003)
///
/// ## Why is this bad?
///
/// Because naive `datetime` objects are treated by many `datetime` methods as
/// local times, it is preferred to use aware datetimes to represent times in
/// UTC. As such, the recommended way to create an object representing the
/// current time in UTC is by calling `datetime.now(timezone.utc)`.
pub(crate) fn call_datetime_utcnow(checker: &mut Checker, func: &Expr, location: TextRange) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "datetime", "utcnow"])
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeUtcnow, location));
    }
}
