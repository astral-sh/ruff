use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct CallDatetimeUtcfromtimestamp;

impl Violation for CallDatetimeUtcfromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.utcfromtimestamp()` is not allowed, use \
             `datetime.datetime.fromtimestamp(ts, tz=)` instead"
        )
    }
}

/// Checks for `datetime.datetime.utcfromtimestamp()`. (DTZ004)
///
/// ## Why is this bad?
///
/// Because naive `datetime` objects are treated by many `datetime` methods as
/// local times, it is preferred to use aware datetimes to represent times in
/// UTC. As such, the recommended way to create an object representing a
/// specific timestamp in UTC is by calling `datetime.fromtimestamp(timestamp,
/// tz=timezone.utc)`.
pub(crate) fn call_datetime_utcfromtimestamp(
    checker: &mut Checker,
    func: &Expr,
    location: TextRange,
) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(
                call_path.as_slice(),
                ["datetime", "datetime", "utcfromtimestamp"]
            )
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeUtcfromtimestamp, location));
    }
}
