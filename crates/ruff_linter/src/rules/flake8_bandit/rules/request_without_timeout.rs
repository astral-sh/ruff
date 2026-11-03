use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::name::QualifiedName;

use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for uses of the Python `requests` or `httpx` module without timeouts.
///
/// ## Why is this bad?
/// The `timeout` parameter is used to set the maximum time to wait for a
/// response from the server. By omitting the `timeout` parameter or
/// explicitly disabling it, the program may hang indefinitely while awaiting a
/// response.
///
/// `requests` has no timeout by default, so both omitting the `timeout` argument
/// and setting it to `None` are flagged. `httpx` applies a default timeout, so only an explicit
/// `timeout=None` is flagged.
///
/// ## Example
/// ```python
/// import requests
///
/// requests.get("https://www.example.com/")
/// ```
///
/// Use instead:
/// ```python
/// import requests
///
/// requests.get("https://www.example.com/", timeout=10)
/// ```
///
/// ## References
/// - [Requests documentation: Timeouts](https://requests.readthedocs.io/en/latest/user/advanced/#timeouts)
/// - [httpx documentation: Timeouts](https://www.python-httpx.org/advanced/timeouts/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.213", category = Category::Security)]
pub(crate) struct RequestWithoutTimeout {
    implicit: bool,
    module: String,
}

impl Violation for RequestWithoutTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithoutTimeout { implicit, module } = self;
        if *implicit {
            format!("Probable use of `{module}` call without timeout")
        } else {
            format!("Probable use of `{module}` call with timeout set to `None`")
        }
    }
}

/// S113
pub(crate) fn request_without_timeout(
    checker: &Checker,
    call: &ast::ExprCall,
    qualified_name: &QualifiedName,
) {
    let module = match qualified_name.segments() {
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

    if let Some(keyword) = call.arguments.find_keyword("timeout") {
        if keyword.value.is_none_literal_expr() {
            checker.report_diagnostic(
                RequestWithoutTimeout {
                    implicit: false,
                    module: module.to_string(),
                },
                keyword.range(),
            );
        }
    } else if module == "requests" {
        checker.report_diagnostic(
            RequestWithoutTimeout {
                implicit: true,
                module: module.to_string(),
            },
            call.func.range(),
        );
    }
}
