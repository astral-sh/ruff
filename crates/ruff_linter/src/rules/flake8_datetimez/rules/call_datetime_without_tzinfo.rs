use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_datetimez::rules::helpers::has_non_none_keyword;

use super::helpers;

/// ## What it does
/// Checks for `datetime` instantiations that lack a `tzinfo` argument.
///
/// ## Why is this bad?
/// `datetime` objects are "naive" by default, in that they do not include
/// timezone information. "Naive" objects are easy to understand, but ignore
/// some aspects of reality, which can lead to subtle bugs. Timezone-aware
/// `datetime` objects are preferred, as they represent a specific moment in
/// time, unlike "naive" objects.
///
/// By providing a `tzinfo` value, a `datetime` can be made timezone-aware.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.datetime(2000, 1, 1, 0, 0, 0)
/// ```
///
/// Use instead:
/// ```python
/// import datetime
///
/// datetime.datetime(2000, 1, 1, 0, 0, 0, tzinfo=datetime.timezone.utc)
/// ```
///
/// Or, for Python 3.11 and later:
/// ```python
/// import datetime
///
/// datetime.datetime(2000, 1, 1, 0, 0, 0, tzinfo=datetime.UTC)
/// ```
#[violation]
pub struct CallDatetimeWithoutTzinfo;

impl Violation for CallDatetimeWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime()` without `tzinfo` argument is not allowed")
    }
}

pub(crate) fn call_datetime_without_tzinfo(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["datetime", "datetime"]))
    {
        return;
    }

    if helpers::parent_expr_is_astimezone(checker) {
        return;
    }

    // No positional arg: keyword is missing or constant None.
    if call.arguments.args.len() < 8 && !has_non_none_keyword(&call.arguments, "tzinfo") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeWithoutTzinfo, call.range()));
        return;
    }

    // Positional arg: is constant None.
    if call.arguments.args.get(7).is_some_and(is_const_none) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeWithoutTzinfo, call.range()));
    }
}
