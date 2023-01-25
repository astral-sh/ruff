use ruff_macros::derive_message_formats;
use rustpython_ast::Stmt;

use crate::ast::types::{Binding, BindingKind, Range};
use crate::define_violation;
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
pub fn runtime_import_in_type_checking_block(
    binding: &Binding,
    blocks: &[&Stmt],
) -> Option<Diagnostic> {
    let full_name = match &binding.kind {
        BindingKind::Importation(.., full_name) => full_name,
        BindingKind::FromImportation(.., full_name) => full_name.as_str(),
        BindingKind::SubmoduleImportation(.., full_name) => full_name,
        _ => return None,
    };

    let defined_in_type_checking = blocks
        .iter()
        .any(|block| Range::from_located(block).contains(&binding.range));
    if defined_in_type_checking {
        if binding.runtime_usage.is_some() {
            return Some(Diagnostic::new(
                RuntimeImportInTypeCheckingBlock {
                    full_name: full_name.to_string(),
                },
                binding.range,
            ));
        }
    }

    None
}
