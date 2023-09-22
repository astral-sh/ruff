use ruff_python_ast::{self as ast, Expr, ExprCall};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `logging.exception()` with `exc_info` set to `False`.
///
/// ## Why is this bad?
/// The `logging.exception()` method captures the exception automatically, but
/// accepts an optional `exc_info` argument to override this behavior. Setting
/// `exc_info` to `False` disables the automatic capture of the exception and
/// stack trace.
///
/// Instead of setting `exc_info` to `False`, prefer `logging.error()`, which
/// has equivalent behavior to `logging.exception()` with `exc_info` set to
/// `False`, but is clearer in intent.
///
/// ## Example
/// ```python
/// logging.exception("...", exc_info=False)
/// ```
///
/// Use instead:
/// ```python
/// logging.error("...")
/// ```
#[violation]
pub struct ExceptionWithoutExcInfo;

impl Violation for ExceptionWithoutExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `logging.exception` with falsy `exc_info`")
    }
}

/// LOG007
pub(crate) fn exception_without_exc_info(checker: &mut Checker, call: &ExprCall) {
    let Expr::Attribute(ast::ExprAttribute { value: _, attr, .. }) = call.func.as_ref() else {
        return;
    };

    if !matches!(
        LoggingLevel::from_attribute(attr.as_str()),
        Some(LoggingLevel::Exception)
    ) {
        return;
    }

    if !logging::is_logger_candidate(
        &call.func,
        checker.semantic(),
        &checker.settings.logger_objects,
    ) {
        return;
    }

    if call
        .arguments
        .find_keyword("exc_info")
        .map(|keyword| &keyword.value)
        .is_some_and(|value| {
            Truthiness::from_expr(value, |id| checker.semantic().is_builtin(id)).is_falsey()
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(ExceptionWithoutExcInfo, call.range()));
    }
}
