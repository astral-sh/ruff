use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::{Decorator, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::helpers::is_pytest_fixture;

/// ## Removed
/// This rule has been removed because it is highly opinionated and may encourage unidiomatic pytest
/// usage. It may be reintroduced in the future under a different category but was not a good fit
/// for the `RUF` category.
///
/// ## What it does
/// Checks for `pytest` fixtures that set the parameter `autouse=True` in the decorator constructor.
///
/// ## Why is this bad?
/// Autouse fixtures are run implicitly, which can make test behavior hard to
/// reason about in general, but especially when defined in `conftest.py` files.
/// Autouse fixtures in `conftest.py` files are automatically run for
/// all tests in the directory structure, which can introduce hidden side effects,
/// make test suites slower, and make debugging difficult.
///
/// Instead, prefer to explicitly request/inject fixtures in tests, test classes,
/// or other fixtures that need them by declaring them in the function parameters.
///
/// ## Example
///
/// ```python
/// import pytest
///
///
/// @pytest.fixture(autouse=True)
/// def my_fixture(): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import pytest
///
///
/// @pytest.fixture()
/// def my_fixture(): ...
///
///
/// def test_foo(my_fixture): ...
/// ```
///
/// ## Note
///
/// This is a pedantic rule that restricts a valid `pytest` pattern. If you choose to
/// enable it, you may want to ignore it outside of `conftest.py` files,
/// as autouse fixtures are most problematic when defined globally.
///
/// You can do this by configuring [`lint.per-file-ignores`][lint.per-file-ignores]:
///
/// ```toml
/// [tool.ruff.lint.per-file-ignores]
/// "!**/conftest.py" = ["RUF076"]
/// ```
///
/// ## References
/// - [`pytest` documentation: Sharing fixtures across classes, modules, packages or session](https://docs.pytest.org/en/stable/how-to/fixtures.html#scope-sharing-fixtures-across-classes-modules-packages-or-session)
/// - [`pytest` documentation: Fixtures can request other fixtures](https://docs.pytest.org/en/stable/how-to/fixtures.html#fixtures-can-request-other-fixtures)
#[derive(ViolationMetadata)]
#[violation_metadata(removed_since = "0.15.20")]
pub(crate) struct PytestFixtureAutouse;

impl Violation for PytestFixtureAutouse {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid using `autouse=True` in `pytest.fixture` decorators".to_string()
    }
}

/// RUF076
pub(crate) fn pytest_fixture_autouse(checker: &Checker, decorators: &[Decorator]) {
    for decorator in decorators {
        if !is_pytest_fixture(decorator, checker) {
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
