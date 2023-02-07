use crate::define_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;

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
