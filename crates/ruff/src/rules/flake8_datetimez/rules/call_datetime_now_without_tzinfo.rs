use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast, Ranged};

use crate::checkers::ast::Checker;
use crate::rules::flake8_datetimez::rules::helpers::has_non_none_keyword;

use super::helpers;

#[violation]
pub struct CallDatetimeNowWithoutTzinfo;

impl Violation for CallDatetimeNowWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime.now()` without `tz` argument is not allowed")
    }
}

/// DTZ005
pub(crate) fn call_datetime_now_without_tzinfo(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["datetime", "datetime", "now"]))
    {
        return;
    }

    if helpers::parent_expr_is_astimezone(checker) {
        return;
    }

    // no args / no args unqualified
    if call.arguments.args.is_empty() && call.arguments.keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, call.range()));
        return;
    }

    // none args
    if call.arguments.args.first().is_some_and(is_const_none) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, call.range()));
        return;
    }

    // wrong keywords / none keyword
    if !call.arguments.keywords.is_empty() && !has_non_none_keyword(&call.arguments, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, call.range()));
    }
}
