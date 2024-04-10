use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Parameters};
use ruff_text_size::Ranged;

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
/// - [`pytest-mock`](https://pypi.org/project/pytest-mock/)
#[violation]
pub struct PytestPatchWithLambda;

impl Violation for PytestPatchWithLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `return_value=` instead of patching with `lambda`")
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

fn check_patch_call(call: &ast::ExprCall, index: usize) -> Option<Diagnostic> {
    if call.arguments.find_keyword("return_value").is_some() {
        return None;
    }

    let ast::ExprLambda {
        parameters,
        body,
        range: _,
    } = call
        .arguments
        .find_argument("new", index)?
        .as_lambda_expr()?;

    // Walk the lambda body. If the lambda uses the arguments, then it's valid.
    if let Some(parameters) = parameters {
        let mut visitor = LambdaBodyVisitor {
            parameters,
            uses_args: false,
        };
        visitor.visit_expr(body);
        if visitor.uses_args {
            return None;
        }
    }

    Some(Diagnostic::new(PytestPatchWithLambda, call.func.range()))
}

/// PT008
pub(crate) fn patch_with_lambda(call: &ast::ExprCall) -> Option<Diagnostic> {
    let name = UnqualifiedName::from_expr(&call.func)?;

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
        check_patch_call(call, 1)
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
        check_patch_call(call, 2)
    } else {
        None
    }
}
