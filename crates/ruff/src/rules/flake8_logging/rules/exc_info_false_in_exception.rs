use ruff_python_ast::ExprCall;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::Truthiness;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `logging.exception()` with `exc_info` set to `False`
///
/// ## Why is this bad?
/// The `exception()` method captures the exception automatically. Disabling this by setting
/// `exc_info=False` is the same as using `error()`, which is clearer and doesn’t need the
/// `exc_info` argument. This rule detects `exception()` calls with an exc_info argument that is
/// falsy.
///
/// ## Example
/// ```python
/// logging.exception("foo", exc_info=False)
/// ```
///
/// Use instead:
/// ```python
/// logging.error("foo")
/// ```
#[violation]
pub struct ExcInfoFalseInException;

impl Violation for ExcInfoFalseInException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `logging.exception` with falsy `exc_info`")
    }
}

/// LOG007
pub(crate) fn exc_info_false_in_exception(checker: &mut Checker, call: &ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(call.func.as_ref())
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["logging", "exception"]))
    {
        if call
            .arguments
            .find_keyword("exc_info")
            .filter(|keyword| {
                Truthiness::from_expr(&keyword.value, |id| checker.semantic().is_builtin(id))
                    .is_falsey()
            })
            .is_some()
        {
            checker
                .diagnostics
                .push(Diagnostic::new(ExcInfoFalseInException, call.range()));
        }
    }
}
