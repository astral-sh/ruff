use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprStringLiteral;

/// ## What it does
/// Checks for hardcoded bindings to all network interfaces (`0.0.0.0`).
///
/// ## Why is this bad?
/// Binding to all network interfaces is insecure as it allows access from
/// unintended interfaces, which may be poorly secured or unauthorized.
///
/// Instead, bind to specific interfaces.
///
/// ## Example
/// ```python
/// ALLOWED_HOSTS = ["0.0.0.0"]
/// ```
///
/// Use instead:
/// ```python
/// ALLOWED_HOSTS = ["127.0.0.1", "localhost"]
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-200](https://cwe.mitre.org/data/definitions/200.html)
#[violation]
pub struct HardcodedBindAllInterfaces;

impl Violation for HardcodedBindAllInterfaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible binding to all interfaces")
    }
}

/// S104
pub(crate) fn hardcoded_bind_all_interfaces(string: &ExprStringLiteral) -> Option<Diagnostic> {
    if string.value.as_str() == "0.0.0.0" {
        Some(Diagnostic::new(HardcodedBindAllInterfaces, string.range))
    } else {
        None
    }
}
