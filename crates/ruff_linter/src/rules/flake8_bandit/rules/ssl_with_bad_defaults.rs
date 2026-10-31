use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::fields as ast;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0", category = Category::Security)]
pub(crate) struct SslWithBadDefaults {
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
pub(crate) fn ssl_with_bad_defaults(checker: &Checker, function_def: &ast::StmtFunctionDef<'_>) {
    for default in function_def
        .parameters
        .iter_non_variadic_params()
        .filter_map(flat::View::<ruff_python_ast::ParameterWithDefault>::default)
    {
        match default.classify() {
            flat::ExprKind::Name(expr_name) if is_insecure_protocol(expr_name.id().as_str()) => {
                let id = expr_name.id();
                let range = expr_name.range();

                checker.report_diagnostic(
                    SslWithBadDefaults {
                        protocol: id.to_string(),
                    },
                    range,
                );
            }
            flat::ExprKind::Attribute(expr_attribute)
                if is_insecure_protocol(expr_attribute.attr().as_str()) =>
            {
                let attr = expr_attribute.attr();
                let range = expr_attribute.range();

                checker.report_diagnostic(
                    SslWithBadDefaults {
                        protocol: attr.to_string(),
                    },
                    range,
                );
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
