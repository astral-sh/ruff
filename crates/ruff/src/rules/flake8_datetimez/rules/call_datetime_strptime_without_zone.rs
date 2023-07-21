use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::has_non_none_keyword;

use crate::checkers::ast::Checker;

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
pub(crate) fn call_datetime_strptime_without_zone(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    location: TextRange,
) {
    if !checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
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
    })) = args.get(1).as_ref()
    {
        if format.contains("%z") {
            return;
        }
    };

    let (Some(grandparent), Some(parent)) = (
        checker.semantic().expr_grandparent(),
        checker.semantic().expr_parent(),
    ) else {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeStrptimeWithoutZone, location));
        return;
    };

    if let Expr::Call(ast::ExprCall { keywords, .. }) = grandparent {
        if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = parent {
            let attr = attr.as_str();
            // Ex) `datetime.strptime(...).astimezone()`
            if attr == "astimezone" {
                return;
            }

            // Ex) `datetime.strptime(...).replace(tzinfo=UTC)`
            if attr == "replace" {
                if has_non_none_keyword(keywords, "tzinfo") {
                    return;
                }
            }
        }
    }

    checker
        .diagnostics
        .push(Diagnostic::new(CallDatetimeStrptimeWithoutZone, location));
}
