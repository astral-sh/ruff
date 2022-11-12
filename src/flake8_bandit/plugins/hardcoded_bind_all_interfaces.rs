use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// S104
pub fn hardcoded_bind_all_interfaces(value: &str, range: &Range) -> Option<Check> {
    if value == "0.0.0.0" {
        Some(Check::new(CheckKind::HardcodedBindAllInterfaces, *range))
    } else {
        None
    }
}
