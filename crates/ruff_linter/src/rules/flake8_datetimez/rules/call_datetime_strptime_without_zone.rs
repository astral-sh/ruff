use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;

use crate::checkers::ast::Checker;

use super::helpers::DatetimeModuleAntipattern;

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
/// object. Follow it with `.replace(tzinfo=<timezone>)` or `.astimezone()`.
///
/// ## Example
/// ```python
/// import datetime
///
/// datetime.datetime.strptime("2022/01/31", "%Y/%m/%d")
/// ```
///
/// Instead, use `.replace(tzinfo=<timezone>)`:
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
#[derive(ViolationMetadata)]
pub(crate) struct CallDatetimeStrptimeWithoutZone(DatetimeModuleAntipattern);

impl Violation for CallDatetimeStrptimeWithoutZone {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CallDatetimeStrptimeWithoutZone(antipattern) = self;
        match antipattern {
            DatetimeModuleAntipattern::NoTzArgumentPassed => {
                "Naive datetime constructed using `datetime.datetime.strptime()` without %z"
                    .to_string()
            }
            DatetimeModuleAntipattern::NonePassedToTzArgument => {
                "`datetime.datetime.strptime(...).replace(tz=None)` used".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let CallDatetimeStrptimeWithoutZone(antipattern) = self;
        let title = match antipattern {
            DatetimeModuleAntipattern::NoTzArgumentPassed => {
                "Call `.replace(tzinfo=<timezone>)` or `.astimezone()` \
                to convert to an aware datetime"
            }
            DatetimeModuleAntipattern::NonePassedToTzArgument => {
                "Pass a `datetime.timezone` object to the `tzinfo` parameter"
            }
        };
        Some(title.to_string())
    }
}

/// DTZ007
pub(crate) fn call_datetime_strptime_without_zone(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::DATETIME) {
        return;
    }

    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["datetime", "datetime", "strptime"]
            )
        })
    {
        return;
    }

    // Does the `strptime` call contain a format string with a timezone specifier?
    if let Some(expr) = call.arguments.args.get(1) {
        match expr {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                if value.to_str().contains("%z") {
                    return;
                }
            }
            Expr::FString(ast::ExprFString { value, .. }) => {
                for f_string_part in value {
                    match f_string_part {
                        ast::FStringPart::Literal(string) => {
                            if string.contains("%z") {
                                return;
                            }
                        }
                        ast::FStringPart::FString(f_string) => {
                            if f_string
                                .elements
                                .literals()
                                .any(|literal| literal.contains("%z"))
                            {
                                return;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let semantic = checker.semantic();
    if let Some(antipattern) = find_antipattern(
        semantic.current_expression_grandparent(),
        semantic.current_expression_parent(),
    ) {
        checker.report_diagnostic(Diagnostic::new(
            CallDatetimeStrptimeWithoutZone(antipattern),
            call.range,
        ));
    }
}

fn find_antipattern(
    grandparent: Option<&Expr>,
    parent: Option<&Expr>,
) -> Option<DatetimeModuleAntipattern> {
    let Some(Expr::Call(ast::ExprCall { arguments, .. })) = grandparent else {
        return Some(DatetimeModuleAntipattern::NoTzArgumentPassed);
    };
    let Some(Expr::Attribute(ast::ExprAttribute { attr, .. })) = parent else {
        return Some(DatetimeModuleAntipattern::NoTzArgumentPassed);
    };
    // Ex) `datetime.strptime(...).astimezone()`
    if attr == "astimezone" {
        return None;
    }
    if attr != "replace" {
        return Some(DatetimeModuleAntipattern::NoTzArgumentPassed);
    }
    match arguments.find_keyword("tzinfo") {
        // Ex) `datetime.strptime(...).replace(tzinfo=None)`
        Some(ast::Keyword {
            value: Expr::NoneLiteral(_),
            ..
        }) => Some(DatetimeModuleAntipattern::NonePassedToTzArgument),
        // Ex) `datetime.strptime(...).replace(tzinfo=...)`
        Some(_) => None,
        // Ex) `datetime.strptime(...).replace(...)` with no `tzinfo` argument
        None => Some(DatetimeModuleAntipattern::NoTzArgumentPassed),
    }
}
