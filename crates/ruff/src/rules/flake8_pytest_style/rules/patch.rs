use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::helpers::includes_arg_name;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Parameters, Ranged};

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

impl<'a, 'b> Visitor<'b> for LambdaBodyVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                if includes_arg_name(id, self.parameters) {
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

    // Walk the lambda body.
    let mut visitor = LambdaBodyVisitor {
        parameters,
        uses_args: false,
    };
    visitor.visit_expr(body);

    if visitor.uses_args {
        None
    } else {
        Some(Diagnostic::new(PytestPatchWithLambda, call.func.range()))
    }
}

/// PT008
pub(crate) fn patch_with_lambda(call: &ast::ExprCall) -> Option<Diagnostic> {
    let call_path = collect_call_path(&call.func)?;

    if matches!(
        call_path.as_slice(),
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
        call_path.as_slice(),
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
