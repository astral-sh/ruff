use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Parameters};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

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
#[violation_metadata(stable_since = "v0.0.208")]
pub(crate) struct PytestPatchWithLambda;

impl Violation for PytestPatchWithLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `return_value=` instead of patching with `lambda`".to_string()
    }
}

/// Visitor that checks references the argument names in the lambda body.
#[derive(Debug)]
struct LambdaBodyVisitor<'a> {
    parameters: &'a Parameters,
    uses_args: bool,
}

impl<'a> Visitor<'a> for LambdaBodyVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                if self.parameters.includes(id) {
                    self.uses_args = true;
                }
            }
            _ => {
                if !self.uses_args {
                    visitor::walk_expr(self, expr);
                }
            }
        }
    }
}

fn check_patch_call(checker: &Checker, call: &ast::ExprCall, index: usize) {
    if call.arguments.find_keyword("return_value").is_some() {
        return;
    }

    let Some(ast::ExprLambda {
        parameters,
        body,
        range: _,
        node_index: _,
    }) = call
        .arguments
        .find_argument_value("new", index)
        .and_then(|expr| expr.as_lambda_expr())
    else {
        return;
    };

    // Walk the lambda body. If the lambda uses the arguments, then it's valid.
    if let Some(parameters) = parameters {
        let mut visitor = LambdaBodyVisitor {
            parameters,
            uses_args: false,
        };
        visitor.visit_expr(body);
        if visitor.uses_args {
            return;
        }
    }

    checker.report_diagnostic(PytestPatchWithLambda, call.func.range());
}

/// PT008
pub(crate) fn patch_with_lambda(checker: &Checker, call: &ast::ExprCall) {
    let Some(name) = UnqualifiedName::from_expr(&call.func) else {
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
