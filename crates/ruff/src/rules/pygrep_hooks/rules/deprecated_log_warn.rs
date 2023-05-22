use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
/// - [Python documentation](https://docs.python.org/3/library/logging.html#logging.Logger.warning)
#[violation]
pub struct DeprecatedLogWarn;

impl Violation for DeprecatedLogWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`warn` is deprecated in favor of `warning`")
    }
}

/// PGH002
pub(crate) fn deprecated_log_warn(checker: &mut Checker, func: &Expr) {
    if checker
        .semantic_model()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["logging", "warn"]
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(DeprecatedLogWarn, func.range()));
    }
}
