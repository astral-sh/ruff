use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

define_violation!(
    pub struct UndefinedExport {
        pub name: String,
    }
);
impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport { name } = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}
