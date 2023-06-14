use ruff_text_size::TextRange;
use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_non_none_keyword, is_const_none};

use crate::checkers::ast::Checker;

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

/// DTZ006
pub(crate) fn call_datetime_fromtimestamp(
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
            matches!(
                call_path.as_slice(),
                ["datetime", "datetime", "fromtimestamp"]
            )
        })
    {
        return;
    }

    // no args / no args unqualified
    if args.len() < 2 && keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, location));
        return;
    }

    // none args
    if args.len() > 1 && is_const_none(&args[1]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, location));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_non_none_keyword(keywords, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, location));
    }
}
