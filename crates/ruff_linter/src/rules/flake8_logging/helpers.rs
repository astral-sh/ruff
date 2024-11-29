use crate::checkers::ast::Checker;
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::ExprCall;

fn exc_info_arg_is_(checker: &mut Checker, call: &ExprCall, boolean: bool) -> bool {
    call.arguments
        .find_keyword("exc_info")
        .map(|keyword| &keyword.value)
        .is_some_and(|value| {
            let truthiness =
                Truthiness::from_expr(value, |id| checker.semantic().has_builtin_binding(id));
            truthiness.into_bool() == Some(boolean)
        })
}

pub(super) fn exc_info_arg_is_falsey(checker: &mut Checker, call: &ExprCall) -> bool {
    exc_info_arg_is_(checker, call, false)
}

pub(super) fn exc_info_arg_is_truey(checker: &mut Checker, call: &ExprCall) -> bool {
    exc_info_arg_is_(checker, call, true)
}

#[inline]
pub(super) fn is_logger_method_name(attr: &str) -> bool {
    matches!(
        attr,
        "debug" | "info" | "warn" | "warning" | "error" | "critical" | "log" | "exception"
    )
}
