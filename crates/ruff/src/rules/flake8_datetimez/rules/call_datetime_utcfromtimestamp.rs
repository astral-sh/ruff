use ruff_python_ast::Expr;
use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for usage of `datetime.datetime.utcfromtimestamp()`.
///
/// ## Why is this bad?
/// Python datetime objects can be naive or timezone-aware. While an aware
/// object represents a specific moment in time, a naive object does not
/// contain enough information to unambiguously locate itself relative to other
/// datetime objects. Since this can lead to errors, it is recommended to
/// always use timezone-aware objects.
///
/// `datetime.datetime.utcfromtimestamp()` returns a naive datetime object;
/// instead, use `datetime.datetime.fromtimestamp(ts, tz=)` to return a
/// timezone-aware object.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.datetime.utcfromtimestamp()
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

pub(crate) fn call_datetime_utcfromtimestamp(
    checker: &mut Checker,
    func: &Expr,
    location: TextRange,
) {
    if !checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["datetime", "datetime", "utcfromtimestamp"]
            )
        })
    {
        return;
    }

    if helpers::parent_expr_is_astimezone(checker) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(CallDatetimeUtcfromtimestamp, location));
}
