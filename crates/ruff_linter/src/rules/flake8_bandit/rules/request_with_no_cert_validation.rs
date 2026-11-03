use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::is_const_false;
use ruff_python_ast::name::QualifiedName;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.213", category = Category::Security)]
pub(crate) struct RequestWithNoCertValidation {
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
    checker: &Checker,
    call: &ast::ExprCall,
    qualified_name: &QualifiedName,
) {
    let target = match qualified_name.segments() {
        [
            "requests",
            "get" | "options" | "head" | "post" | "put" | "patch" | "delete" | "request",
        ] => "requests",
        [
            "httpx",
            "get" | "options" | "head" | "post" | "put" | "patch" | "delete" | "request" | "stream"
            | "Client" | "AsyncClient",
        ] => "httpx",
        _ => return,
    };

    if let Some(keyword) = call.arguments.find_keyword("verify")
        && is_const_false(&keyword.value)
    {
        checker.report_diagnostic(
            RequestWithNoCertValidation {
                string: target.to_string(),
            },
            keyword.range(),
        );
    }
}
