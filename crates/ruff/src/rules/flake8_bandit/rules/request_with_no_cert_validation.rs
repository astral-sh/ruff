use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_false, SimpleCallArgs};

use crate::checkers::ast::Checker;

#[violation]
pub struct RequestWithNoCertValidation {
    string: String,
}

impl Violation for RequestWithNoCertValidation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithNoCertValidation { string } = self;
        format!(
            "Probable use of `{string}` call with `verify=False` disabling SSL certificate checks"
        )
    }
}

/// S501
pub(crate) fn request_with_no_cert_validation(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(target) = checker
        .semantic()
        .resolve_call_path(func)
        .and_then(|call_path| match call_path.as_slice() {
            ["requests", "get" | "options" | "head" | "post" | "put" | "patch" | "delete"] => {
                Some("requests")
            }
            ["httpx", "get" | "options" | "head" | "post" | "put" | "patch" | "delete" | "request"
            | "stream" | "Client" | "AsyncClient"] => Some("httpx"),
            _ => None,
        })
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(verify_arg) = call_args.keyword_argument("verify") {
            if is_const_false(verify_arg) {
                checker.diagnostics.push(Diagnostic::new(
                    RequestWithNoCertValidation {
                        string: target.to_string(),
                    },
                    verify_arg.range(),
                ));
            }
        }
    }
}
