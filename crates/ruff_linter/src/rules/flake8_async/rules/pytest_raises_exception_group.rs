use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_pytest_style::rules::is_pytest_raises;

/// ## What it does
/// Checks for `pytest.raises` calls that expect `ExceptionGroup` or
/// `BaseExceptionGroup`.
///
/// ## Why is this bad?
/// `pytest.raises(ExceptionGroup)` checks the group's type, but not the types
/// or number of exceptions it contains. A test can therefore pass even if the
/// group contains unexpected exceptions.
///
/// Use `pytest.RaisesGroup` to specify the expected exceptions and any nested
/// groups.
///
/// ## Example
/// ```python
/// import pytest
///
/// with pytest.raises(ExceptionGroup):
///     raise ExceptionGroup("errors", [ValueError()])
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
/// with pytest.RaisesGroup(ValueError):
///     raise ExceptionGroup("errors", [ValueError()])
/// ```
///
/// ## Known problems
/// `pytest.RaisesGroup` requires an exact number of expected exceptions. Tests
/// that allow an unspecified number can instead use `pytest.raises` and
/// validate the group's contents with a `check` callback or subsequent
/// assertions. This rule also reports those calls.
///
/// ## References
/// - [Python documentation: Exception groups](https://docs.python.org/3/builtins/exceptions.html#exception-groups)
/// - [pytest documentation: `pytest.RaisesGroup`](https://docs.pytest.org/en/stable/reference/reference.html#pytest.RaisesGroup)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct PytestRaisesExceptionGroup;

impl Violation for PytestRaisesExceptionGroup {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `pytest.RaisesGroup` instead of `pytest.raises` for exception groups".to_string()
    }
}

/// ASYNC401
pub(crate) fn pytest_raises_exception_group(checker: &Checker, call: &ExprCall) {
    if !is_pytest_raises(&call.func, checker.semantic()) {
        return;
    }

    let Some(expected_exception) = call.arguments.args.first().or_else(|| {
        call.arguments
            .find_keyword("expected_exception")
            .map(|keyword| &keyword.value)
    }) else {
        return;
    };

    if contains_exception_group(expected_exception, checker.semantic()) {
        checker.report_diagnostic(PytestRaisesExceptionGroup, call.func.range());
    }
}

fn contains_exception_group(expr: &Expr, semantic: &SemanticModel) -> bool {
    if let Expr::Tuple(tuple) = expr {
        return tuple
            .iter()
            .any(|element| contains_exception_group(element, semantic));
    }

    semantic
        .resolve_qualified_name(map_subscript(expr))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                [
                    "" | "builtins" | "exceptiongroup",
                    "ExceptionGroup" | "BaseExceptionGroup"
                ]
            )
        })
}
