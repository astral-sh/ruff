use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path, SimpleCallArgs};
use crate::ast::types::Range;
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
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
    let call_path = dealias_call_path(collect_call_paths(func), import_aliases);
    let call_args = SimpleCallArgs::new(args, keywords);

    for func_name in &REQUESTS_HTTP_VERBS {
        if match_call_path(&call_path, "requests", func_name, from_imports) {
            if let Some(verify_arg) = call_args.get_argument("verify", None) {
                if let ExprKind::Constant {
                    value: Constant::Bool(false),
                    ..
                } = &verify_arg.node
                {
                    return Some(Diagnostic::new(
                        violations::RequestWithNoCertValidation("requests".to_string()),
                        Range::from_located(verify_arg),
                    ));
                }
            }
        }
    }

    for func_name in &HTTPX_METHODS {
        if match_call_path(&call_path, "httpx", func_name, from_imports) {
            if let Some(verify_arg) = call_args.get_argument("verify", None) {
                if let ExprKind::Constant {
                    value: Constant::Bool(false),
                    ..
                } = &verify_arg.node
                {
                    return Some(Diagnostic::new(
                        violations::RequestWithNoCertValidation("httpx".to_string()),
                        Range::from_located(verify_arg),
                    ));
                }
            }
        }
    }
    None
}
