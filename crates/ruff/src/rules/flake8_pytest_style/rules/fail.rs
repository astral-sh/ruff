use ruff_python_ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::CallArguments;

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
#[violation]
pub struct PytestFailWithoutMessage;

impl Violation for PytestFailWithoutMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No message passed to `pytest.fail()`")
    }
}

pub(crate) fn fail_call(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if is_pytest_fail(func, checker.semantic()) {
        let call_args = CallArguments::new(args, keywords);

        // Allow either `pytest.fail(reason="...")` (introduced in pytest 7.0) or
        // `pytest.fail(msg="...")` (deprecated in pytest 7.0)
        let msg = call_args
            .argument("reason", 0)
            .or_else(|| call_args.argument("msg", 0));

        if let Some(msg) = msg {
            if is_empty_or_null_string(msg) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PytestFailWithoutMessage, func.range()));
            }
        } else {
            checker
                .diagnostics
                .push(Diagnostic::new(PytestFailWithoutMessage, func.range()));
        }
    }
}
