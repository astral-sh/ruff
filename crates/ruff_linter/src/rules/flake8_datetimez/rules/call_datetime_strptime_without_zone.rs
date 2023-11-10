use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_datetimez::rules::helpers::has_non_none_keyword;

/// ## What it does
/// Checks for uses of `datetime.datetime.strptime()` that lead to naive
/// datetime objects.
///
/// ## Why is this bad?
/// Python datetime objects can be naive or timezone-aware. While an aware
/// object represents a specific moment in time, a naive object does not
/// contain enough information to unambiguously locate itself relative to other
/// datetime objects. Since this can lead to errors, it is recommended to
/// always use timezone-aware objects.
///
/// `datetime.datetime.strptime()` without `%z` returns a naive datetime
/// object. Follow it with `.replace(tzinfo=)` or `.astimezone()`.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.datetime.strptime("2022/01/31", "%Y/%m/%d")
/// ```
///
/// Instead, use `.replace(tzinfo=)`:
/// ```python
/// import datetime
///
/// datetime.datetime.strptime("2022/01/31", "%Y/%m/%d").replace(
///     tzinfo=datetime.timezone.utc
/// )
/// ```
///
/// Or, use `.astimezone()`:
/// ```python
/// import datetime
///
/// datetime.datetime.strptime("2022/01/31", "%Y/%m/%d").astimezone(datetime.timezone.utc)
/// ```
///
/// On Python 3.11 and later, `datetime.timezone.utc` can be replaced with
/// `datetime.UTC`.
///
/// ## References
/// - [Python documentation: Aware and Naive Objects](https://docs.python.org/3/library/datetime.html#aware-and-naive-objects)
/// - [Python documentation: `strftime()` and `strptime()` Behavior](https://docs.python.org/3/library/datetime.html#strftime-and-strptime-behavior)
#[violation]
pub struct CallDatetimeStrptimeWithoutZone;

impl Violation for CallDatetimeStrptimeWithoutZone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.strptime()` without %z must be followed by \
             `.replace(tzinfo=)` or `.astimezone()`"
        )
    }
}

/// DTZ007
pub(crate) fn call_datetime_strptime_without_zone(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| {
            matches!(call_path.as_slice(), ["datetime", "datetime", "strptime"])
        })
    {
        return;
    }

    // Does the `strptime` call contain a format string with a timezone specifier?
    if let Some(Expr::StringLiteral(ast::ExprStringLiteral { value: format, .. })) =
        call.arguments.args.get(1).as_ref()
    {
        if format.contains("%z") {
            return;
        }
    };

    let (Some(grandparent), Some(parent)) = (
        checker.semantic().current_expression_grandparent(),
        checker.semantic().current_expression_parent(),
    ) else {
        checker.diagnostics.push(Diagnostic::new(
            CallDatetimeStrptimeWithoutZone,
            call.range(),
        ));
        return;
    };

    if let Expr::Call(ast::ExprCall { arguments, .. }) = grandparent {
        if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = parent {
            let attr = attr.as_str();
            // Ex) `datetime.strptime(...).astimezone()`
            if attr == "astimezone" {
                return;
            }

            // Ex) `datetime.strptime(...).replace(tzinfo=UTC)`
            if attr == "replace" {
                if has_non_none_keyword(arguments, "tzinfo") {
                    return;
                }
            }
        }
    }

    checker.diagnostics.push(Diagnostic::new(
        CallDatetimeStrptimeWithoutZone,
        call.range(),
    ));
}
