use ruff_python_ast::{ExceptHandler, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_bandit::helpers::is_untyped_exception;

/// ## What it does
/// Checks for uses of the `try`-`except`-`pass` pattern.
///
/// ## Why is this bad?
/// The `try`-`except`-`pass` pattern suppresses all exceptions. Suppressing
/// exceptions may hide errors that could otherwise reveal unexpected behavior,
/// security vulnerabilities, or malicious activity. Instead, consider logging
/// the exception.
///
/// ## Example
/// ```python
/// try:
///     ...
/// except Exception:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// try:
///     ...
/// except Exception as exc:
///     logging.exception("Exception occurred")
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-703](https://cwe.mitre.org/data/definitions/703.html)
/// - [Python documentation: `logging`](https://docs.python.org/3/library/logging.html)
#[violation]
pub struct TryExceptPass;

impl Violation for TryExceptPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`try`-`except`-`pass` detected, consider logging the exception")
    }
}

/// S110
pub(crate) fn try_except_pass(
    checker: &mut Checker,
    except_handler: &ExceptHandler,
    type_: Option<&Expr>,
    body: &[Stmt],
    check_typed_exception: bool,
) {
    if matches!(body, [Stmt::Pass(_)]) {
        if check_typed_exception || is_untyped_exception(type_, checker.semantic()) {
            checker
                .diagnostics
                .push(Diagnostic::new(TryExceptPass, except_handler.range()));
        }
    }
}
