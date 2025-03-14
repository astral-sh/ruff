use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::in_dunder_method;

/// ## What it does
/// Checks for bare `raise` statements outside of exception handlers.
///
/// ## Why is this bad?
/// A bare `raise` statement without an exception object will re-raise the last
/// exception that was active in the current scope, and is typically used
/// within an exception handler to re-raise the caught exception.
///
/// If a bare `raise` is used outside of an exception handler, it will generate
/// an error due to the lack of an active exception.
///
/// Note that a bare `raise` within a  `finally` block will work in some cases
/// (namely, when the exception is raised within the `try` block), but should
/// be avoided as it can lead to confusing behavior.
///
/// ## Example
/// ```python
/// from typing import Any
///
///
/// def is_some(obj: Any) -> bool:
///     if obj is None:
///         raise
/// ```
///
/// Use instead:
/// ```python
/// from typing import Any
///
///
/// def is_some(obj: Any) -> bool:
///     if obj is None:
///         raise ValueError("`obj` cannot be `None`")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MisplacedBareRaise;

impl Violation for MisplacedBareRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Bare `raise` statement is not inside an exception handler".to_string()
    }
}

/// PLE0704
pub(crate) fn misplaced_bare_raise(checker: &Checker, raise: &ast::StmtRaise) {
    if raise.exc.is_some() {
        return;
    }

    if checker.semantic().in_exception_handler() {
        return;
    }

    if in_dunder_method("__exit__", checker.semantic(), checker.settings) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(MisplacedBareRaise, raise.range()));
}
