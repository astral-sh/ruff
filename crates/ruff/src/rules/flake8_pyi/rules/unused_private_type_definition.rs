use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_semantic::Binding;

#[violation]
pub struct UnusedPrivateTypeVar {
    name: String,
}

impl Violation for UnusedPrivateTypeVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypeVar { name } = self;
        format!("TypeVar `{name}` is never used")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var(binding: &Binding, locator: &Locator) -> Option<Diagnostic> {
    if !binding.kind.is_assignment() {
        return None;
    }
    if !binding.is_private_type_var() {
        return None;
    }
    if dbg!(binding.is_used()) {
        return None;
    }

    dbg!(binding.name(locator), binding.range, &binding.references);
    Some(Diagnostic::new(
        UnusedPrivateTypeVar {
            name: binding.name(locator).to_string(),
        },
        binding.range,
    ))
}
