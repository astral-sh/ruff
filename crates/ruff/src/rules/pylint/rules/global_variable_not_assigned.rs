use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

define_violation!(
    pub struct GlobalVariableNotAssigned {
        pub name: String,
    }
);
impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned { name } = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}
