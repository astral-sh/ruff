use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_ast as ast;
use ruff_python_semantic::Modules;

use crate::checkers::ast::Checker;

use super::helpers::{self, DatetimeModuleAntipattern};

/// ## What it does
/// Checks for usages of `datetime.datetime.now()` that do not specify a timezone.
///
/// ## Why is this bad?
/// Python datetime objects can be naive or timezone-aware. While an aware
/// object represents a specific moment in time, a naive object does not
/// contain enough information to unambiguously locate itself relative to other
/// datetime objects. Since this can lead to errors, it is recommended to
/// always use timezone-aware objects.
///
/// `datetime.datetime.now()` or `datetime.datetime.now(tz=None)` returns a naive
/// datetime object. Instead, use `datetime.datetime.now(tz=<timezone>)` to create
/// a timezone-aware object.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.datetime.now()
/// ```
///
/// Use instead:
/// ```python
/// import datetime
///
/// datetime.datetime.now(tz=datetime.timezone.utc)
/// ```
///
/// Or, for Python 3.11 and later:
/// ```python
/// import datetime
///
/// datetime.datetime.now(tz=datetime.UTC)
/// ```
///
/// ## References
/// - [Python documentation: Aware and Naive Objects](https://docs.python.org/3/library/datetime.html#aware-and-naive-objects)
#[derive(ViolationMetadata)]
pub(crate) struct CallDatetimeNowWithoutTzinfo(DatetimeModuleAntipattern);

impl Violation for CallDatetimeNowWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CallDatetimeNowWithoutTzinfo(antipattern) = self;
        match antipattern {
            DatetimeModuleAntipattern::NoTzArgumentPassed => {
                "`datetime.datetime.now()` called without a `tz` argument".to_string()
            }
            DatetimeModuleAntipattern::NonePassedToTzArgument => {
                "`tz=None` passed to `datetime.datetime.now()`".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Pass a `datetime.timezone` object to the `tz` parameter".to_string())
    }
}

pub(crate) fn call_datetime_now_without_tzinfo(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::DATETIME) {
        return;
    }

    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["datetime", "datetime", "now"])
        })
    {
        return;
    }

    if helpers::followed_by_astimezone(checker) {
        return;
    }

    let antipattern = match call.arguments.find_argument_value("tz", 0) {
        Some(ast::Expr::NoneLiteral(_)) => DatetimeModuleAntipattern::NonePassedToTzArgument,
        Some(_) => return,
        None => DatetimeModuleAntipattern::NoTzArgumentPassed,
    };

    checker.report_diagnostic(Diagnostic::new(
        CallDatetimeNowWithoutTzinfo(antipattern),
        call.range,
    ));
}
