use rustpython_ast::{Expr, ExprKind, Keyword};

use super::helpers::{get_all_argument_names, SimpleCallArgs};
use crate::ast::helpers::compose_call_path;
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checks::{Check, CheckKind};

const PATCH_NAMES: &[&str] = &[
    "mocker.patch",
    "class_mocker.patch",
    "module_mocker.patch",
    "package_mocker.patch",
    "session_mocker.patch",
    "mock.patch",
    "unittest.mock.patch",
];

const PATCH_OBJECT_NAMES: &[&str] = &[
    "mocker.patch.object",
    "class_mocker.patch.object",
    "module_mocker.patch.object",
    "package_mocker.patch.object",
    "session_mocker.patch.object",
    "mock.patch.object",
    "unittest.mock.patch.object",
];

#[derive(Default)]
/// Visitor that checks references the argument names in the lambda body.
struct LambdaBodyVisitor {
    names: Vec<String>,
    uses_args: bool,
}

impl<'a, 'b> Visitor<'b> for LambdaBodyVisitor
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Name { id, .. } => {
                if self.names.contains(id) {
                    self.uses_args = true;
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

fn check_patch_call(
    call: &Expr,
    args: &Vec<Expr>,
    keywords: &Vec<Keyword>,
    new_arg_number: usize,
) -> Option<Check> {
    let simple_args = SimpleCallArgs::new(args, keywords);
    if simple_args.get_argument("return_value", None).is_some() {
        return None;
    }

    if let Some(new_arg) = simple_args.get_argument("new", Some(new_arg_number)) {
        if let ExprKind::Lambda { args, body } = &new_arg.node {
            let lambda_arg_names = get_all_argument_names(args);
            // Walk the lambda body
            let mut visitor = LambdaBodyVisitor {
                names: lambda_arg_names,
                uses_args: false,
            };
            visitor.visit_expr(body);

            if !visitor.uses_args {
                return Some(Check::new(
                    CheckKind::PatchWithLambda,
                    Range::from_located(call),
                ));
            }
        }
    }
    None
}

pub fn patch_with_lambda(call: &Expr, args: &Vec<Expr>, keywords: &Vec<Keyword>) -> Option<Check> {
    if let Some(call_path) = compose_call_path(call) {
        if PATCH_NAMES.contains(&call_path.as_str()) {
            return check_patch_call(call, args, keywords, 1);
        } else if PATCH_OBJECT_NAMES.contains(&call_path.as_str()) {
            return check_patch_call(call, args, keywords, 2);
        }
    }
    None
}
