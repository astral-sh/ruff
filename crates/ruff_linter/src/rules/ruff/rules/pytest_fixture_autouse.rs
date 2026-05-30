use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{is_const_true, map_callable};
use ruff_python_ast::{Decorator, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `pytest` fixtures that set the parameter `autouse=True` in the decorator constructor.
///
/// ## Why is this bad?
/// Autouse fixtures are run implicitly, which can make test behavior hard to
/// reason about in general, but especially when defined in `conftest.py` files.
/// When defined in a `conftest.py` file, autouse fixtures are automatically run for
/// all tests in the directory structure, which can introduce hidden side effects,
/// make test suites slower, and make debugging difficult.
///
/// Instead, prefer to explicitly request/inject fixtures in tests, test classes,
/// or other fixtures (by declaring them in the function parameters) that need them.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// @pytest.fixture(autouse=True)
/// def my_fixture():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture():
///     ...
///
///
/// def test_foo(my_fixture):
///     ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.15")]
pub(crate) struct PytestFixtureAutouse;

impl Violation for PytestFixtureAutouse {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid using `autouse=True` in `pytest.fixture` decorators".to_string()
    }
}

/// RUF076
pub(crate) fn pytest_fixture_autouse(checker: &Checker, decorators: &[Decorator]) {
    let semantic = checker.semantic();
    for decorator in decorators {
        // Resolve the qualified name to see if it is a pytest.fixture
        let is_fixture = semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["pytest", "fixture"])
            });

        if !is_fixture {
            continue;
        }

        // Check if decorator has argument autouse=True
        let Expr::Call(call) = &decorator.expression else {
            continue;
        };

        if let Some(keyword) = call.arguments.find_keyword("autouse") {
            if is_const_true(&keyword.value) {
                checker.report_diagnostic(PytestFixtureAutouse, keyword.range());
            }
        }
    }
}
