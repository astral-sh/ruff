use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprCall;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions which have default arguments set to broken SSL/TLS
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
/// def func(version=ssl.PROTOCOL_TLSv1):
/// ```
///
/// Use instead:
/// ```python
/// import ssl
///
/// def func(version=ssl.PROTOCOL_TLSv1_2):
/// ```
#[violation]
pub struct SslWithBadDefaults {
    protocol: String
}

impl Violation for SslWithBadDefaults {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Argument default set to insecure SSL protocol: {}", self.protocol)
    }
}

/// S503
pub(crate) fn ssl_with_bad_defaults(checker: &mut Checker, call: &ExprCall) {}
