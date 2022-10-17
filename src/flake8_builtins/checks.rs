use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_builtins::types::ShadowingType;
use crate::python::builtins::BUILTINS;

/// Check builtin name shadowing.
pub fn builtin_shadowing(name: &str, location: Range, node_type: ShadowingType) -> Option<Check> {
    if BUILTINS.contains(&name) {
        Some(Check::new(
            match node_type {
                ShadowingType::Variable => CheckKind::BuiltinVariableShadowing(name.to_string()),
                ShadowingType::Argument => CheckKind::BuiltinArgumentShadowing(name.to_string()),
                ShadowingType::Attribute => CheckKind::BuiltinAttributeShadowing(name.to_string()),
            },
            location,
        ))
    } else {
        None
    }
}
