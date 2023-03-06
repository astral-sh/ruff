use ruff_macros::{derive_message_formats, violation};

use crate::violation::Violation;

#[violation]
pub struct GlobalVariableNotAssigned {
    pub name: String,
}

impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned { name } = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}
