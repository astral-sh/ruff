use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct HardcodedBindAllInterfaces;

impl Violation for HardcodedBindAllInterfaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible binding to all interfaces")
    }
}

/// S104
pub(crate) fn hardcoded_bind_all_interfaces(value: &str, range: TextRange) -> Option<Diagnostic> {
    if value == "0.0.0.0" {
        Some(Diagnostic::new(HardcodedBindAllInterfaces, range))
    } else {
        None
    }
}
