use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

define_violation!(
    pub struct NonlocalWithoutBinding {
        pub name: String,
    }
);
impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding { name } = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}
