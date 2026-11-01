use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::{Modules, SemanticModel};
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
/// In pytest 8.4.0 and later, use `pytest.RaisesGroup` to specify the expected
/// exceptions and any nested groups.
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
/// This rule does not check which version of pytest a project uses, so it also
/// reports calls in projects that use a version of pytest older than 8.4.0,
/// where `pytest.RaisesGroup` is not available.
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
        "Prefer `pytest.RaisesGroup` over an exception group type".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        if self.nested {
            Some("Use a nested `pytest.RaisesGroup` to specify the expected exceptions".to_string())
        } else {
            Some("Use `pytest.RaisesGroup` to specify the expected exceptions".to_string())
        }
    }
}

/// ASYNC401
pub(crate) fn pytest_raises_exception_group(checker: &Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::PYTEST) {
        return;
    }

    let Some(qualified_name) = checker
        .semantic()
        .resolve_qualified_name(map_subscript(&call.func))
    else {
        return;
    };

    let is_exception_group = |expr: &&Expr| contains_exception_group(expr, checker.semantic());
    let (exception, nested) = match qualified_name.segments() {
        ["pytest", "raises"] => (
            call.arguments
                .find_argument_value("expected_exception", 0)
                .filter(is_exception_group),
            false,
        ),
        ["pytest", "RaisesExc"] => (
            call.arguments.args.first().filter(is_exception_group),
            false,
        ),
        ["pytest", "RaisesGroup"] => (call.arguments.args.iter().find(is_exception_group), true),
        ["pytest", "mark", "xfail"] => (
            call.arguments
                .find_keyword("raises")
                .map(|keyword| &keyword.value)
                .filter(is_exception_group),
            false,
        ),
        _ => return,
    };

    if let Some(exception) = exception {
        checker.report_diagnostic(PytestRaisesExceptionGroup { nested }, exception.range());
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
