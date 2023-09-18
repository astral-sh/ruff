use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::is_const_false;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for HTTPS requests that disable SSL certificate checks.
///
/// ## Why is this bad?
/// If SSL certificates are not verified, an attacker could perform a "man in
/// the middle" attack by intercepting and modifying traffic between the client
/// and server.
///
/// ## Example
/// ```python
/// import requests
///
/// requests.get("https://www.example.com", verify=False)
/// ```
///
/// Use instead:
/// ```python
/// import requests
///
/// requests.get("https://www.example.com")  # By default, `verify=True`.
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-295](https://cwe.mitre.org/data/definitions/295.html)
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
pub(crate) fn request_with_no_cert_validation(checker: &mut Checker, call: &ast::ExprCall) {
    if let Some(target) = checker
        .semantic()
        .resolve_call_path(&call.func)
        .and_then(|call_path| match call_path.as_slice() {
            ["requests", "get" | "options" | "head" | "post" | "put" | "patch" | "delete"] => {
                Some("requests")
            }
            ["httpx", "get" | "options" | "head" | "post" | "put" | "patch" | "delete" | "request"
            | "stream" | "Client" | "AsyncClient"] => Some("httpx"),
            _ => None,
        })
    {
        if let Some(keyword) = call.arguments.find_keyword("verify") {
            if is_const_false(&keyword.value) {
                checker.diagnostics.push(Diagnostic::new(
                    RequestWithNoCertValidation {
                        string: target.to_string(),
                    },
                    keyword.range(),
                ));
            }
        }
    }
}
