use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct UndefinedName {
    pub name: String,
}

impl Violation for UndefinedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedName { name } = self;
        format!("Undefined name `{name}`")
    }
}
