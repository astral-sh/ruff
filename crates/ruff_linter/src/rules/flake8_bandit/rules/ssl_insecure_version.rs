use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::analyze::typing::find_assigned_value;

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
    protocol: String,
}

impl Violation for SslInsecureVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Call made with insecure SSL protocol: {}", self.protocol)
    }
}

const INSECURSE_SSL_PROTOCOLS: &[&str] = &[
    "PROTOCOL_SSLv2",
    "PROTOCOL_SSLv3",
    "PROTOCOL_TLSv1",
    "PROTOCOL_TLSv1_1",
    "SSLv2_METHOD",
    "SSLv23_METHOD",
    "SSLv3_METHOD",
    "TLSv1_METHOD",
    "TLSv1_1_METHOD",
];

/// S502
pub(crate) fn ssl_insecure_version(checker: &mut Checker, call: &ExprCall) {
    let Some(call_path) = checker.semantic().resolve_call_path(call.func.as_ref()) else {
        return;
    };

    let keywords = match call_path.as_slice() {
        &["ssl", "wrap_socket"] => vec!["ssl_version"],
        &["OpenSSL", "SSL", "Context"] => vec!["method"],
        _ => vec!["ssl_version", "method"],
    };

    let mut violations: Vec<Diagnostic> = vec![];

    for arg in keywords {
        let Some(keyword) = call.arguments.find_keyword(arg) else {
            return;
        };
        match &keyword.value {
            Expr::Name(ast::ExprName { id, .. }) => {
                let Some(val) = find_assigned_value(id, checker.semantic()) else {
                    continue;
                };
                println!("ASSIGNED VALUE: {:?}", val);
                match val {
                    Expr::Name(ast::ExprName { id, .. }) => {
                        if INSECURSE_SSL_PROTOCOLS.contains(&id.as_str()) {
                            violations.push(Diagnostic::new(
                                SslInsecureVersion {
                                    protocol: id.to_string(),
                                },
                                keyword.range,
                            ));
                        }
                    }
                    Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                        if INSECURSE_SSL_PROTOCOLS.contains(&attr.as_str()) {
                            violations.push(Diagnostic::new(
                                SslInsecureVersion {
                                    protocol: attr.to_string(),
                                },
                                keyword.range,
                            ))
                        }
                    }
                    _ => {
                        continue;
                    }
                }
            }
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                if INSECURSE_SSL_PROTOCOLS.contains(&attr.as_str()) {
                    violations.push(Diagnostic::new(
                        SslInsecureVersion {
                            protocol: attr.to_string(),
                        },
                        keyword.range,
                    ))
                }
            }
            _ => {
                return;
            }
        }
    }

    for violation in violations {
        checker.diagnostics.push(violation)
    }
}
