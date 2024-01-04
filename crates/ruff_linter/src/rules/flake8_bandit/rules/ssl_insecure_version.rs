use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls to Python methods with parameters that indicate the used broken SSL/TLS
/// protocol versions.
///
/// ## Why is this bad?
/// Several highly publicized exploitable flaws have been discovered in all versions of SSL and
/// early versions of TLS. It is strongly recommended that use of the following known broken
/// protocol versions be avoided:
///     - SSL v2
///     - SSL v3
///     - TLS v1
///     - TLS v1.1
///
/// ## Example
/// ```python
/// import ssl
///
/// ssl.wrap_socket(ssl_version=ssl.PROTOCOL_TLSv1)
/// ```
///
/// Use instead:
/// ```python
/// import ssl
///
/// ssl.wrap_socket(ssl_version=ssl.PROTOCOL_TLSv1_2)
/// ```
#[violation]
pub struct SslInsecureVersion {
    protocol: String
}

impl Violation for SslInsecureVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Call made with insecure SSL protocol: {}", self.protocol)
    }
}

/// S502
pub(crate) fn ssl_insecure_version(checker: &mut Checker, call: &ExprCall) {}
