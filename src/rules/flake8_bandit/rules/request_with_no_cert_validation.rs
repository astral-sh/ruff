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
    if let Some(target) = checker.resolve_call_path(func).and_then(|call_path| {
        if call_path.len() == 2 {
            if call_path[0] == "requests" && REQUESTS_HTTP_VERBS.contains(&call_path[1]) {
                return Some("requests");
            }
            if call_path[0] == "httpx" && HTTPX_METHODS.contains(&call_path[1]) {
                return Some("httpx");
            }
        }
        None
    }) {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(verify_arg) = call_args.get_argument("verify", None) {
            if let ExprKind::Constant {
                value: Constant::Bool(false),
                ..
            } = &verify_arg.node
            {
                checker.diagnostics.push(Diagnostic::new(
                    violations::RequestWithNoCertValidation(target.to_string()),
                    Range::from_located(verify_arg),
                ));
            }
        }
    }
}
