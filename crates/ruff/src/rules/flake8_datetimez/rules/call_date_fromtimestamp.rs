use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct CallDateFromtimestamp;

impl Violation for CallDateFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.date.fromtimestamp()` is not allowed, use \
             `datetime.datetime.fromtimestamp(ts, tz=).date()` instead"
        )
    }
}

/// Checks for `datetime.date.fromtimestamp()`. (DTZ012)
///
/// ## Why is this bad?
///
/// It uses the system local timezone.
/// Use `datetime.datetime.fromtimestamp(, tz=).date()` instead.
pub(crate) fn call_date_fromtimestamp(checker: &mut Checker, func: &Expr, location: TextRange) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "date", "fromtimestamp"])
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDateFromtimestamp, location));
    }
}
