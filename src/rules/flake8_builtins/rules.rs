use rustpython_ast::Located;

use super::types::ShadowingType;
use crate::ast::types::Range;
use crate::python::builtins::BUILTINS;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::violations;

/// Check builtin name shadowing.
pub fn builtin_shadowing<T>(
    name: &str,
    located: &Located<T>,
    node_type: ShadowingType,
    ignorelist: &[String],
) -> Option<Diagnostic> {
    if BUILTINS.contains(&name) && !ignorelist.contains(&name.to_string()) {
        Some(Diagnostic::new::<DiagnosticKind>(
            match node_type {
                ShadowingType::Variable => {
                    violations::BuiltinVariableShadowing(name.to_string()).into()
                }
                ShadowingType::Argument => {
                    violations::BuiltinArgumentShadowing(name.to_string()).into()
                }
                ShadowingType::Attribute => {
                    violations::BuiltinAttributeShadowing(name.to_string()).into()
                }
            },
            Range::from_located(located),
        ))
    } else {
        None
    }
}
