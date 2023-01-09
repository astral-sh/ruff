use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S104
pub fn hardcoded_bind_all_interfaces(value: &str, range: &Range) -> Option<Diagnostic> {
    if value == "0.0.0.0" {
        Some(Diagnostic::new(
            violations::HardcodedBindAllInterfaces,
            *range,
        ))
    } else {
        None
    }
}
