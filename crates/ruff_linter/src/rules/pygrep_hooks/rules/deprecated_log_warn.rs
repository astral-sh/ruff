use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::analyze::logging;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for usages of the deprecated `warn` method from the `logging` module.
///
/// ## Why is this bad?
/// The `warn` method is deprecated. Use `warning` instead.
///
/// ## Example
/// ```python
/// import logging
///
///
/// def foo():
///     logging.warn("Something happened")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
///
/// def foo():
///     logging.warning("Something happened")
/// ```
///
/// ## References
/// - [Python documentation: `logger.Logger.warning`](https://docs.python.org/3/library/logging.html#logging.Logger.warning)
#[violation]
pub struct DeprecatedLogWarn;

impl Violation for DeprecatedLogWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`warn` is deprecated in favor of `warning`")
    }
}

/// PGH002
pub(crate) fn deprecated_log_warn(checker: &mut Checker, call: &ExprCall) {
    match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            if !logging::is_logger_candidate(
                &call.func,
                checker.semantic(),
                &checker.settings.logger_objects,
            ) {
                return;
            }
            if !matches!(
                LoggingLevel::from_attribute(attr.as_str()),
                Some(LoggingLevel::Warn)
            ) {
                return;
            }
        }
        Expr::Name(_) => {
            if !checker
                .semantic()
                .resolve_call_path(call.func.as_ref())
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["logging", "warn"]))
            {
                return;
            }
        }
        _ => return,
    }

    checker
        .diagnostics
        .push(Diagnostic::new(DeprecatedLogWarn, call.func.range()));
}
