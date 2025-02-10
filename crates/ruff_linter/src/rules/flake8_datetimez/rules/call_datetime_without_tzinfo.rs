use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use ruff_python_ast as ast;
use ruff_python_semantic::Modules;

use crate::checkers::ast::Checker;

use super::helpers::{self, DatetimeModuleAntipattern};

/// ## What it does
/// Checks for `datetime` instantiations that do not specify a timezone.
///
/// ## Why is this bad?
/// `datetime` objects are "naive" by default, in that they do not include
/// timezone information. "Naive" objects are easy to understand, but ignore
/// some aspects of reality, which can lead to subtle bugs. Timezone-aware
/// `datetime` objects are preferred, as they represent a specific moment in
/// time, unlike "naive" objects.
///
/// By providing a non-`None` value for `tzinfo`, a `datetime` can be made
/// timezone-aware.
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
/// Or, on Python 3.11 and later:
/// ```python
/// import datetime
///
/// datetime.datetime(2000, 1, 1, 0, 0, 0, tzinfo=datetime.UTC)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct CallDatetimeWithoutTzinfo(DatetimeModuleAntipattern);

impl Violation for CallDatetimeWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CallDatetimeWithoutTzinfo(antipattern) = self;
        match antipattern {
            DatetimeModuleAntipattern::NoTzArgumentPassed => {
                "`datetime.datetime()` called without a `tzinfo` argument".to_string()
            }
            DatetimeModuleAntipattern::NonePassedToTzArgument => {
                "`tzinfo=None` passed to `datetime.datetime()`".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Pass a `datetime.timezone` object to the `tzinfo` parameter".to_string())
    }
}

pub(crate) fn call_datetime_without_tzinfo(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::DATETIME) {
        return;
    }

    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["datetime", "datetime"]))
    {
        return;
    }

    if helpers::followed_by_astimezone(checker) {
        return;
    }

    let antipattern = match call.arguments.find_argument_value("tzinfo", 7) {
        Some(ast::Expr::NoneLiteral(_)) => DatetimeModuleAntipattern::NonePassedToTzArgument,
        Some(_) => return,
        None => DatetimeModuleAntipattern::NoTzArgumentPassed,
    };

    checker.report_diagnostic(Diagnostic::new(
        CallDatetimeWithoutTzinfo(antipattern),
        call.range,
    ));
}
