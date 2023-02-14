use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::builtins::BUILTINS;
use rustpython_parser::ast::Located;

use super::types::ShadowingType;
use crate::ast::types::Range;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::violation::Violation;

define_violation!(
    pub struct BuiltinVariableShadowing {
        pub name: String,
    }
);
impl Violation for BuiltinVariableShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinVariableShadowing { name } = self;
        format!("Variable `{name}` is shadowing a python builtin")
    }
}

define_violation!(
    pub struct BuiltinArgumentShadowing {
        pub name: String,
    }
);
impl Violation for BuiltinArgumentShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinArgumentShadowing { name } = self;
        format!("Argument `{name}` is shadowing a python builtin")
    }
}

define_violation!(
    pub struct BuiltinAttributeShadowing {
        pub name: String,
    }
);
impl Violation for BuiltinAttributeShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinAttributeShadowing { name } = self;
        format!("Class attribute `{name}` is shadowing a python builtin")
    }
}

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
                ShadowingType::Variable => BuiltinVariableShadowing {
                    name: name.to_string(),
                }
                .into(),
                ShadowingType::Argument => BuiltinArgumentShadowing {
                    name: name.to_string(),
                }
                .into(),
                ShadowingType::Attribute => BuiltinAttributeShadowing {
                    name: name.to_string(),
                }
                .into(),
            },
            Range::from_located(located),
        ))
    } else {
        None
    }
}
