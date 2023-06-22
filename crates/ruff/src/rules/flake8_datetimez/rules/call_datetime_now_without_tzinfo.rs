use ruff_text_size::TextRange;
use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_non_none_keyword, is_const_none};

use crate::checkers::ast::Checker;

#[violation]
pub struct CallDatetimeNowWithoutTzinfo;

impl Violation for CallDatetimeNowWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime.now()` without `tz` argument is not allowed")
    }
}

/// DTZ005
pub(crate) fn call_datetime_now_without_tzinfo(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: TextRange,
) {
    if !checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["datetime", "datetime", "now"])
        })
    {
        return;
    }

    // no args / no args unqualified
    if args.is_empty() && keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, location));
        return;
    }

    // none args
    if !args.is_empty() && is_const_none(&args[0]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, location));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_non_none_keyword(keywords, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, location));
    }
}
