use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::helpers::{is_empty_or_null_string, is_pytest_fail};

/// ## What it does
/// Checks for `pytest.fail` calls without a message.
///
/// ## Why is this bad?
/// `pytest.fail` calls without a message make it harder to understand and debug test failures.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// def test_foo():
///     pytest.fail()
///
///
/// def test_bar():
///     pytest.fail("")
///
///
/// def test_baz():
///     pytest.fail(reason="")
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// def test_foo():
///     pytest.fail("...")
///
///
/// def test_bar():
///     pytest.fail(reason="...")
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.fail`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-fail)
#[derive(ViolationMetadata)]
pub(crate) struct PytestFailWithoutMessage;

impl Violation for PytestFailWithoutMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        "No message passed to `pytest.fail()`".to_string()
    }
}

pub(crate) fn fail_call(checker: &Checker, call: &ast::ExprCall) {
    if is_pytest_fail(&call.func, checker.semantic()) {
        // Allow either `pytest.fail(reason="...")` (introduced in pytest 7.0) or
        // `pytest.fail(msg="...")` (deprecated in pytest 7.0)
        if call
            .arguments
            .find_argument_value("reason", 0)
            .or_else(|| call.arguments.find_argument_value("msg", 0))
            .is_none_or(is_empty_or_null_string)
        {
            checker.report_diagnostic(Diagnostic::new(PytestFailWithoutMessage, call.func.range()));
        }
    }
}
