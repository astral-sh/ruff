use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::Scope;
use ruff_text_size::TextRange;

#[violation]
pub struct UndefinedExport {
    name: String,
}

impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport { name } = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}

/// F822
pub(crate) fn undefined_export(names: &[&str], range: TextRange, scope: &Scope) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if !scope.uses_star_imports() {
        for name in names {
            if !scope.defines(name) {
                diagnostics.push(Diagnostic::new(
                    UndefinedExport {
                        name: (*name).to_string(),
                    },
                    range,
                ));
            }
        }
    }
    diagnostics
}
