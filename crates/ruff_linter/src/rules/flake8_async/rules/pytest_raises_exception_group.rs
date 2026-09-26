use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `pytest.raises`, `pytest.RaisesExc`, `pytest.RaisesGroup`, and
/// `pytest.mark.xfail` calls that expect `ExceptionGroup` or `BaseExceptionGroup`.
///
/// ## Why is this bad?
/// `pytest.raises(ExceptionGroup)` checks the group's type, but not the types
/// or number of exceptions it contains. A test can therefore pass even if the
/// group contains unexpected exceptions.
///
/// Use `pytest.RaisesGroup` to specify the expected exceptions and any nested
/// groups. In `pytest.RaisesGroup`, use a nested matcher such as
/// `pytest.RaisesGroup(pytest.RaisesGroup(ValueError))`. In `pytest.mark.xfail`,
/// use `raises=pytest.RaisesGroup(ValueError)`.
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
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Pedantic)]
pub(crate) struct PytestRaisesExceptionGroup {
    nested: bool,
}

impl Violation for PytestRaisesExceptionGroup {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.nested {
            "Use a nested `pytest.RaisesGroup` to specify the expected exceptions".to_string()
        } else {
            "Use `pytest.RaisesGroup` to specify expected exceptions in the group".to_string()
        }
    }
}

/// ASYNC401
pub(crate) fn pytest_raises_exception_group(checker: &Checker, call: &ExprCall) {
    let Some(qualified_name) = checker
        .semantic()
        .resolve_qualified_name(map_subscript(&call.func))
    else {
        return;
    };

    let is_exception_group = |expr| contains_exception_group(expr, checker.semantic());
    let expects_exception_group = match qualified_name.segments() {
        ["pytest", "raises"] => call
            .arguments
            .find_argument_value("expected_exception", 0)
            .is_some_and(is_exception_group),
        ["pytest", "RaisesExc"] => call.arguments.args.first().is_some_and(is_exception_group),
        ["pytest", "RaisesGroup"] => {
            if let Some(exception) = call
                .arguments
                .args
                .iter()
                .find(|exception| is_exception_group(exception))
            {
                checker.report_diagnostic(
                    PytestRaisesExceptionGroup { nested: true },
                    exception.range(),
                );
            }
            return;
        }
        ["pytest", "mark", "xfail"] => call
            .arguments
            .find_keyword("raises")
            .is_some_and(|keyword| is_exception_group(&keyword.value)),
        _ => false,
    };

    if expects_exception_group {
        checker.report_diagnostic(
            PytestRaisesExceptionGroup { nested: false },
            call.func.range(),
        );
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
