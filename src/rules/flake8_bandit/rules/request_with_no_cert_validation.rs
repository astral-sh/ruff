use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

const REQUESTS_HTTP_VERBS: [&str; 7] = ["get", "options", "head", "post", "put", "patch", "delete"];
const HTTPX_METHODS: [&str; 11] = [
    "get",
    "options",
    "head",
    "post",
    "put",
    "patch",
    "delete",
    "request",
    "stream",
    "Client",
    "AsyncClient",
];

/// S501
pub fn request_with_no_cert_validation(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(call_path) = checker.resolve_call_path(func) {
        let call_args = SimpleCallArgs::new(args, keywords);
        for func_name in &REQUESTS_HTTP_VERBS {
            if call_path == ["requests", func_name] {
                if let Some(verify_arg) = call_args.get_argument("verify", None) {
                    if let ExprKind::Constant {
                        value: Constant::Bool(false),
                        ..
                    } = &verify_arg.node
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::RequestWithNoCertValidation("requests".to_string()),
                            Range::from_located(verify_arg),
                        ));
                    }
                }
                return;
            }
        }
        for func_name in &HTTPX_METHODS {
            if call_path == ["httpx", func_name] {
                if let Some(verify_arg) = call_args.get_argument("verify", None) {
                    if let ExprKind::Constant {
                        value: Constant::Bool(false),
                        ..
                    } = &verify_arg.node
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::RequestWithNoCertValidation("httpx".to_string()),
                            Range::from_located(verify_arg),
                        ));
                    }
                }
                return;
            }
        }
    }
}
