use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::name::UnqualifiedName;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for mocked calls that use a dummy `lambda` function instead of
/// `return_value`.
///
/// ## Why is this bad?
/// When patching calls, an explicit `return_value` better conveys the intent
/// than a `lambda` function, assuming the `lambda` does not use the arguments
/// passed to it.
///
/// `return_value` is also robust to changes in the patched function's
/// signature, and enables additional assertions to verify behavior. For
/// example, `return_value` allows for verification of the number of calls or
/// the arguments passed to the patched function via `assert_called_once_with`
/// and related methods.
///
/// ## Example
/// ```python
/// def test_foo(mocker):
///     mocker.patch("module.target", lambda x, y: 7)
/// ```
///
/// Use instead:
/// ```python
/// def test_foo(mocker):
///     mocker.patch("module.target", return_value=7)
///
///     # If the lambda makes use of the arguments, no diagnostic is emitted.
///     mocker.patch("module.other_target", lambda x, y: x)
/// ```
///
/// ## References
/// - [Python documentation: `unittest.mock.patch`](https://docs.python.org/3/library/unittest.mock.html#unittest.mock.patch)
/// - [PyPI: `pytest-mock`](https://pypi.org/project/pytest-mock/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.208", category = Category::Complexity)]
pub(crate) struct PytestPatchWithLambda;

impl Violation for PytestPatchWithLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `return_value=` instead of patching with `lambda`".to_string()
    }
}

fn check_patch_call(checker: &Checker, call: ExprCall<'_>, index: usize) {
    if call.arguments().find_keyword("return_value").is_some() {
        return;
    }

    let Some(lambda) = call
        .arguments()
        .find_argument_value("new", index)
        .and_then(ruff_python_ast::Expr::as_lambda_expr)
    else {
        return;
    };

    if let Some(parameters) = lambda.parameters()
        && any_over_expr(lambda.body(), |expr| {
            expr.as_name_expr()
                .is_some_and(|name| parameters.includes(name.id()))
        })
    {
        return;
    }

    checker.report_diagnostic(PytestPatchWithLambda, call.func().range());
}

/// PT008
pub(crate) fn patch_with_lambda(checker: &Checker, call: ExprCall<'_>) {
    let Some(name) = UnqualifiedName::from_expr(call.func()) else {
        return;
    };

    if matches!(
        name.segments(),
        [
            "mocker"
                | "class_mocker"
                | "module_mocker"
                | "package_mocker"
                | "session_mocker"
                | "mock",
            "patch"
        ] | ["unittest", "mock", "patch"]
    ) {
        check_patch_call(checker, call, 1);
    } else if matches!(
        name.segments(),
        [
            "mocker"
                | "class_mocker"
                | "module_mocker"
                | "package_mocker"
                | "session_mocker"
                | "mock",
            "patch",
            "object"
        ] | ["unittest", "mock", "patch", "object"]
    ) {
        check_patch_call(checker, call, 2);
    }
}
