use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::{Binding, BindingKind, ExecutionContext};
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct RuntimeImportInTypeCheckingBlock {
        pub full_name: String,
    }
);
impl Violation for RuntimeImportInTypeCheckingBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move import `{}` out of type-checking block. Import is used for more than type \
             hinting.",
            self.full_name
        )
    }
}

/// TCH004
pub fn runtime_import_in_type_checking_block(binding: &Binding) -> Option<Diagnostic> {
    let full_name = match &binding.kind {
        BindingKind::Importation(.., full_name) => full_name,
        BindingKind::FromImportation(.., full_name) => full_name.as_str(),
        BindingKind::SubmoduleImportation(.., full_name) => full_name,
        _ => return None,
    };

    if matches!(binding.context, ExecutionContext::Typing) && binding.runtime_usage.is_some() {
        Some(Diagnostic::new(
            RuntimeImportInTypeCheckingBlock {
                full_name: full_name.to_string(),
            },
            binding.range,
        ))
    } else {
        None
    }
}
