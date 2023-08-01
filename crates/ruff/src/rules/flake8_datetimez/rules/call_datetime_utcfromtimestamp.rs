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
/// `datetime` objects are "naive" by default, in that they do not include
/// timezone information. "Naive" objects are easy to understand, but ignore
/// some aspects of reality, which can lead to subtle bugs. Timezone-aware
/// `datetime` objects are preferred, as they represent a specific moment in
/// time, unlike "naive" objects.
///
/// `datetime.datetime.utcfromtimestamp()` creates a "naive" object; instead,
/// use `datetime.datetime.fromtimestamp(timestamp, tz=timezone.utc)`.
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
        .map_or(false, |call_path| {
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
