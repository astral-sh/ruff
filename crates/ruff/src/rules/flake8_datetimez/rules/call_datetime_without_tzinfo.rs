use ruff_python_ast::{Expr, Keyword};
use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{has_non_none_keyword, is_const_none};

use crate::checkers::ast::Checker;

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

pub(crate) fn call_datetime_without_tzinfo(
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
            matches!(call_path.as_slice(), ["datetime", "datetime"])
        })
    {
        return;
    }

    if helpers::parent_expr_is_astimezone(checker) {
        return;
    }

    // No positional arg: keyword is missing or constant None.
    if args.len() < 8 && !has_non_none_keyword(keywords, "tzinfo") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeWithoutTzinfo, location));
        return;
    }

    // Positional arg: is constant None.
    if args.len() >= 8 && is_const_none(&args[7]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeWithoutTzinfo, location));
    }
}
