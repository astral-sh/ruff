use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, Ranged};

use crate::checkers::ast::Checker;
use crate::rules::flake8_datetimez::rules::helpers::has_non_none_keyword;

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
    if let Some(Expr::Constant(ast::ExprConstant {
        value: Constant::Str(format),
        kind: None,
        range: _,
    })) = call.arguments.args.get(1).as_ref()
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
