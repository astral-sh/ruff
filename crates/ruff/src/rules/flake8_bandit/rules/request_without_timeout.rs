use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_none, SimpleCallArgs};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the Python `requests` module without a timeout.
///
/// ## Why is this bad?
/// The `timeout` parameter is used to set the maximum time to wait for a
/// response from the server. Without a timeout, the program may hang
/// indefinitely waiting for a response.
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
#[violation]
pub struct RequestWithoutTimeout {
    implicit: bool,
}

impl Violation for RequestWithoutTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithoutTimeout { implicit } = self;
        if *implicit {
            format!("Probable use of requests call without timeout")
        } else {
            format!("Probable use of requests call with timeout set to `None`")
        }
    }
}

/// S113
pub(crate) fn request_without_timeout(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(
                call_path.as_slice(),
                [
                    "requests",
                    "get" | "options" | "head" | "post" | "put" | "patch" | "delete"
                ]
            )
        })
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(timeout) = call_args.keyword_argument("timeout") {
            if is_const_none(timeout) {
                checker.diagnostics.push(Diagnostic::new(
                    RequestWithoutTimeout { implicit: false },
                    timeout.range(),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                RequestWithoutTimeout { implicit: true },
                func.range(),
            ));
        }
    }
}
