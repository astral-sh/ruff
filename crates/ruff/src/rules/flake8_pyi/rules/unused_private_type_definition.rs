use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct UnusedPrivateTypeVar;

impl Violation for UnusedPrivateTypeVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("TODO")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var() {}
