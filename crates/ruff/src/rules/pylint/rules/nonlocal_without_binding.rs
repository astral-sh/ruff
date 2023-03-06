use ruff_macros::{derive_message_formats, violation};

use crate::violation::Violation;

#[violation]
pub struct NonlocalWithoutBinding {
    pub name: String,
}

impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding { name } = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}
