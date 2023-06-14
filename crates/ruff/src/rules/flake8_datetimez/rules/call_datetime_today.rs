use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct CallDatetimeToday;

impl Violation for CallDatetimeToday {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.today()` is not allowed, use \
             `datetime.datetime.now(tz=)` instead"
        )
    }
}

/// Checks for `datetime.datetime.today()`. (DTZ002)
///
/// ## Why is this bad?
///
/// It uses the system local timezone.
/// Use `datetime.datetime.now(tz=)` instead.
pub(crate) fn call_datetime_today(checker: &mut Checker, func: &Expr, location: TextRange) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "datetime", "today"])
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeToday, location));
    }
}
