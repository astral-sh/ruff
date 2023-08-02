use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast, Ranged};

use crate::checkers::ast::Checker;
use crate::rules::flake8_datetimez::rules::helpers::has_non_none_keyword;

use super::helpers;

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
pub(crate) fn call_datetime_fromtimestamp(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["datetime", "datetime", "fromtimestamp"]
            )
        })
    {
        return;
    }

    if helpers::parent_expr_is_astimezone(checker) {
        return;
    }

    // no args / no args unqualified
    if call.arguments.args.len() < 2 && call.arguments.keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, call.range()));
        return;
    }

    // none args
    if call.arguments.args.len() > 1 && is_const_none(&call.arguments.args[1]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, call.range()));
        return;
    }

    // wrong keywords / none keyword
    if !call.arguments.keywords.is_empty() && !has_non_none_keyword(&call.arguments, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, call.range()));
    }
}
