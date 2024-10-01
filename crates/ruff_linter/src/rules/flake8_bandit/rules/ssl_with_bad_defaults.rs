use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, StmtFunctionDef};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions with default arguments set to insecure SSL
/// and TLS protocol versions.
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
/// ## Example
///
/// ```python
/// import ssl
///
///
/// def func(version=ssl.PROTOCOL_TLSv1): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import ssl
///
///
/// def func(version=ssl.PROTOCOL_TLSv1_2): ...
/// ```
#[violation]
pub struct SslWithBadDefaults {
    protocol: String,
}

impl Violation for SslWithBadDefaults {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SslWithBadDefaults { protocol } = self;
        format!("Argument default set to insecure SSL protocol: `{protocol}`")
    }
}

/// S503
pub(crate) fn ssl_with_bad_defaults(checker: &mut Checker, function_def: &StmtFunctionDef) {
    for default in function_def
        .parameters
        .iter_non_variadic_params()
        .filter_map(|param| param.default.as_deref())
    {
        match default {
            Expr::Name(ast::ExprName { id, range, .. }) => {
                if is_insecure_protocol(id.as_str()) {
                    checker.diagnostics.push(Diagnostic::new(
                        SslWithBadDefaults {
                            protocol: id.to_string(),
                        },
                        *range,
                    ));
                }
            }
            Expr::Attribute(ast::ExprAttribute { attr, range, .. }) => {
                if is_insecure_protocol(attr.as_str()) {
                    checker.diagnostics.push(Diagnostic::new(
                        SslWithBadDefaults {
                            protocol: attr.to_string(),
                        },
                        *range,
                    ));
                }
            }
            _ => {}
        }
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
