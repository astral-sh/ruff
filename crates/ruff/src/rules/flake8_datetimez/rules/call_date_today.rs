use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct CallDateToday;

impl Violation for CallDateToday {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.date.today()` is not allowed, use \
             `datetime.datetime.now(tz=).date()` instead"
        )
    }
}

/// Checks for `datetime.date.today()`. (DTZ011)
///
/// ## Why is this bad?
///
/// It uses the system local timezone.
/// Use `datetime.datetime.now(tz=).date()` instead.
pub(crate) fn call_date_today(checker: &mut Checker, func: &Expr, location: TextRange) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "date", "today"])
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDateToday, location));
    }
}
