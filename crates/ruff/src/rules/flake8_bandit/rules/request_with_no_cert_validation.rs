use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RequestWithNoCertValidation {
        pub string: String,
    }
);
impl Violation for RequestWithNoCertValidation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithNoCertValidation { string } = self;
        format!(
            "Probable use of `{string}` call with `verify=False` disabling SSL certificate checks"
        )
    }
}

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
                    RequestWithNoCertValidation {
                        string: target.to_string(),
                    },
                    Range::from_located(verify_arg),
                ));
            }
        }
    }
}
