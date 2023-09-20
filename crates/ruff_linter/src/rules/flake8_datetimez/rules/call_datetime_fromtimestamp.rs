use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_datetimez::rules::helpers::has_non_none_keyword;

use super::helpers;

/// ## What it does
/// Checks for usage of `datetime.datetime.fromtimestamp()` without a `tz`
/// argument.
///
/// ## Why is this bad?
/// Python datetime objects can be naive or timezone-aware. While an aware
/// object represents a specific moment in time, a naive object does not
/// contain enough information to unambiguously locate itself relative to other
/// datetime objects. Since this can lead to errors, it is recommended to
/// always use timezone-aware objects.
///
/// `datetime.datetime.fromtimestamp(ts)` returns a naive datetime object.
/// Instead, use `datetime.datetime.fromtimestamp(ts, tz=)` to return a
/// timezone-aware object.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.datetime.fromtimestamp(946684800)
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
pub struct CallDatetimeFromtimestamp;

impl Violation for CallDatetimeFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.fromtimestamp()` without `tz` argument is not allowed"
        )
    }
}

pub(crate) fn call_datetime_fromtimestamp(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["datetime", "datetime", "fromtimestamp"]
            )
        })
    {
        return;
    }

    if helpers::parent_expr_is_astimezone(checker) {
        return;
    }

    // no args / no args unqualified
    if call.arguments.args.len() < 2 && call.arguments.keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, call.range()));
        return;
    }

    // none args
    if call.arguments.args.len() > 1 && is_const_none(&call.arguments.args[1]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, call.range()));
        return;
    }

    // wrong keywords / none keyword
    if !call.arguments.keywords.is_empty() && !has_non_none_keyword(&call.arguments, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, call.range()));
    }
}
