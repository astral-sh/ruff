use crate::ast::types::Range;
use crate::registry::Check;
use crate::violations;

/// S104
pub fn hardcoded_bind_all_interfaces(value: &str, range: &Range) -> Option<Check> {
    if value == "0.0.0.0" {
        Some(Check::new(violations::HardcodedBindAllInterfaces, *range))
    } else {
        None
    }
}
