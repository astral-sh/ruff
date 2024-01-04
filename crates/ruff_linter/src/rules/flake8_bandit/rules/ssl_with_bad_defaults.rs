use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, StmtFunctionDef};
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
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import ssl
///
/// def func(version=ssl.PROTOCOL_TLSv1_2):
///     ...
/// ```
#[violation]
pub struct SslWithBadDefaults {
    protocol: String,
}

impl Violation for SslWithBadDefaults {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Argument default set to insecure SSL protocol: {}",
            self.protocol
        )
    }
}

const INSECURE_SSL_PROTOCOLS: &[&str] = &[
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

/// S503
pub(crate) fn ssl_with_bad_defaults(checker: &mut Checker, function_def: &StmtFunctionDef) {
    function_def
        .parameters
        .posonlyargs
        .iter()
        .chain(
            function_def
                .parameters
                .args
                .iter()
                .chain(function_def.parameters.kwonlyargs.iter()),
        )
        .for_each(|param| {
            if let Some(default) = &param.default {
                check_default_arg_for_insecure_ssl_violation(default, checker);
            }
        });
}

fn check_default_arg_for_insecure_ssl_violation(expr: &Expr, checker: &mut Checker) {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => {
            if INSECURE_SSL_PROTOCOLS.contains(&id.as_str()) {
                checker.diagnostics.push(Diagnostic::new(
                    SslWithBadDefaults {
                        protocol: id.to_string(),
                    },
                    expr.range(),
                ));
            }
        }
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            if INSECURE_SSL_PROTOCOLS.contains(&attr.as_str()) {
                checker.diagnostics.push(Diagnostic::new(
                    SslWithBadDefaults {
                        protocol: attr.to_string(),
                    },
                    expr.range(),
                ));
            }
        }
        _ => {}
    }
}
