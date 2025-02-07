use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls to `ssl.wrap_socket()` without an `ssl_version`.
///
/// ## Why is this bad?
/// This method is known to provide a default value that maximizes
/// compatibility, but permits use of insecure protocols.
///
/// ## Example
/// ```python
/// import ssl
///
/// ssl.wrap_socket()
/// ```
///
/// Use instead:
/// ```python
/// import ssl
///
/// ssl.wrap_socket(ssl_version=ssl.PROTOCOL_TLSv1_2)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SslWithNoVersion;

impl Violation for SslWithNoVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`ssl.wrap_socket` called without an `ssl_version``".to_string()
    }
}

/// S504
pub(crate) fn ssl_with_no_version(checker: &Checker, call: &ExprCall) {
    if checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["ssl", "wrap_socket"]))
    {
        if call.arguments.find_keyword("ssl_version").is_none() {
            checker.report_diagnostic(Diagnostic::new(SslWithNoVersion, call.range()));
        }
    }
}
