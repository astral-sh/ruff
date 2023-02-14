use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

define_violation!(
    pub struct UndefinedName {
        pub name: String,
    }
);
impl Violation for UndefinedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedName { name } = self;
        format!("Undefined name `{name}`")
    }
}
