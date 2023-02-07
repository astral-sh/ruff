use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use crate::ast::helpers::{collect_arg_names, compose_call_path, SimpleCallArgs};
use crate::ast::types::Range;
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PatchWithLambda;
);
impl Violation for PatchWithLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `return_value=` instead of patching with `lambda`")
    }
}

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
struct LambdaBodyVisitor<'a> {
    names: FxHashSet<&'a str>,
    uses_args: bool,
}

impl<'a, 'b> Visitor<'b> for LambdaBodyVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Name { id, .. } => {
                if self.names.contains(&id.as_str()) {
                    self.uses_args = true;
                }
            }
            _ => visitor::walk_expr(self, expr),
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
    if simple_args.get_argument("return_value", None).is_some() {
        return None;
    }

    if let Some(new_arg) = simple_args.get_argument("new", Some(new_arg_number)) {
        if let ExprKind::Lambda { args, body } = &new_arg.node {
            // Walk the lambda body.
            let mut visitor = LambdaBodyVisitor {
                names: collect_arg_names(args),
                uses_args: false,
            };
            visitor.visit_expr(body);

            if !visitor.uses_args {
                return Some(Diagnostic::new(PatchWithLambda, Range::from_located(call)));
            }
        }
    }
    None
}

pub fn patch_with_lambda(call: &Expr, args: &[Expr], keywords: &[Keyword]) -> Option<Diagnostic> {
    if let Some(call_path) = compose_call_path(call) {
        if PATCH_NAMES.contains(&call_path.as_str()) {
            check_patch_call(call, args, keywords, 1)
        } else if PATCH_OBJECT_NAMES.contains(&call_path.as_str()) {
            check_patch_call(call, args, keywords, 2)
        } else {
            None
        }
    } else {
        None
    }
}
