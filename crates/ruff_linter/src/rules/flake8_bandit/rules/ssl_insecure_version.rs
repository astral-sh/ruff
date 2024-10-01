use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function calls with parameters that indicate the use of insecure
/// SSL and TLS protocol versions.
///
/// ## Why is this bad?
/// Several highly publicized exploitable flaws have been discovered in all
/// versions of SSL and early versions of TLS. The following versions are
/// considered insecure, and should be avoided:
/// - SSL v2
/// - SSL v3
/// - TLS v1
/// - TLS v1.1
///
/// This method supports detection on the Python's built-in `ssl` module and
/// the `pyOpenSSL` module.
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
    protocol: String,
}

impl Violation for SslInsecureVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SslInsecureVersion { protocol } = self;
        format!("Call made with insecure SSL protocol: `{protocol}`")
    }
}

/// S502
pub(crate) fn ssl_insecure_version(checker: &mut Checker, call: &ExprCall) {
    let Some(keyword) = checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .and_then(|qualified_name| match qualified_name.segments() {
            ["ssl", "wrap_socket"] => Some("ssl_version"),
            ["OpenSSL", "SSL", "Context"] => Some("method"),
            _ => None,
        })
    else {
        return;
    };

    let Some(keyword) = call.arguments.find_keyword(keyword) else {
        return;
    };

    match &keyword.value {
        Expr::Name(ast::ExprName { id, .. }) => {
            if is_insecure_protocol(id) {
                checker.diagnostics.push(Diagnostic::new(
                    SslInsecureVersion {
                        protocol: id.to_string(),
                    },
                    keyword.range(),
                ));
            }
        }
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            if is_insecure_protocol(attr) {
                checker.diagnostics.push(Diagnostic::new(
                    SslInsecureVersion {
                        protocol: attr.to_string(),
                    },
                    keyword.range(),
                ));
            }
        }
        _ => {}
    }
}

/// Returns `true` if the given protocol name is insecure.
fn is_insecure_protocol(name: &str) -> bool {
    matches!(
        name,
        "PROTOCOL_SSLv2"
            | "PROTOCOL_SSLv3"
            | "PROTOCOL_TLSv1"
            | "PROTOCOL_TLSv1_1"
            | "SSLv2_METHOD"
            | "SSLv23_METHOD"
            | "SSLv3_METHOD"
            | "TLSv1_METHOD"
            | "TLSv1_1_METHOD"
    )
}
