use rustpython_parser::ast::{self, Arguments, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::helpers::{includes_arg_name, SimpleCallArgs};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

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
    arguments: &'a Arguments,
    uses_args: bool,
}

impl<'a, 'b> Visitor<'b> for LambdaBodyVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                if includes_arg_name(id, self.arguments) {
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

fn check_patch_call(
    call: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    new_arg_number: usize,
) -> Option<Diagnostic> {
    let simple_args = SimpleCallArgs::new(args, keywords);
    if simple_args.keyword_argument("return_value").is_some() {
        return None;
    }

    if let Some(Expr::Lambda(ast::ExprLambda {
        args,
        body,
        range: _,
    })) = simple_args.argument("new", new_arg_number)
    {
        // Walk the lambda body.
        let mut visitor = LambdaBodyVisitor {
            arguments: args,
            uses_args: false,
        };
        visitor.visit_expr(body);

        if !visitor.uses_args {
            return Some(Diagnostic::new(PytestPatchWithLambda, call.range()));
        }
    }

    None
}

pub(crate) fn patch_with_lambda(
    call: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    let call_path = collect_call_path(call)?;

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
        check_patch_call(call, args, keywords, 1)
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
        check_patch_call(call, args, keywords, 2)
    } else {
        None
    }
}
