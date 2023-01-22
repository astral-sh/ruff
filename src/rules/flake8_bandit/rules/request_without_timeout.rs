use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

const HTTP_VERBS: [&str; 7] = ["get", "options", "head", "post", "put", "patch", "delete"];

/// S113
pub fn request_without_timeout(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        HTTP_VERBS
            .iter()
            .any(|func_name| call_path.as_slice() == ["requests", func_name])
    }) {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(timeout_arg) = call_args.get_argument("timeout", None) {
            if let Some(timeout) = match &timeout_arg.node {
                ExprKind::Constant {
                    value: value @ Constant::None,
                    ..
                } => Some(value.to_string()),
                _ => None,
            } {
                checker.diagnostics.push(Diagnostic::new(
                    violations::RequestWithoutTimeout(Some(timeout)),
                    Range::from_located(timeout_arg),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                violations::RequestWithoutTimeout(None),
                Range::from_located(func),
            ));
        }
    }
}
