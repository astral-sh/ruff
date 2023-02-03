use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use crate::define_simple_violation;
use ruff_macros::derive_message_formats;

define_simple_violation!(
    HardcodedBindAllInterfaces,
    "Possible binding to all interfaces"
);

/// S104
pub fn hardcoded_bind_all_interfaces(value: &str, range: &Range) -> Option<Diagnostic> {
    if value == "0.0.0.0" {
        Some(Diagnostic::new(HardcodedBindAllInterfaces, *range))
    } else {
        None
    }
}
