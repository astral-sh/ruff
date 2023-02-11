use crate::ast::types::{Range, Scope};
use crate::registry::Diagnostic;
use ruff_macros::{define_violation, derive_message_formats};
use std::path::Path;

use crate::violation::Violation;

define_violation!(
    pub struct UndefinedExport {
        pub name: String,
    }
);
impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport { name } = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}

/// F822
pub fn undefined_export(
    names: &[&str],
    range: &Range,
    path: &Path,
    scope: &Scope,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if !scope.import_starred && !path.ends_with("__init__.py") {
        for name in names {
            if !scope.bindings.contains_key(name) {
                diagnostics.push(Diagnostic::new(
                    UndefinedExport {
                        name: (*name).to_string(),
                    },
                    *range,
                ));
            }
        }
    }
    diagnostics
}
