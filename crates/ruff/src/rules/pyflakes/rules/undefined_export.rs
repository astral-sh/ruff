use std::path::Path;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::scope::Scope;
use ruff_python_ast::types::Range;

#[violation]
pub struct UndefinedExport {
    pub name: String,
}

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
            if !scope.defines(name) {
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
