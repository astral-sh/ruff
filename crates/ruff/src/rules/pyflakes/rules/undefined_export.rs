use crate::define_violation;

use crate::violation::Violation;
use ruff_macros::derive_message_formats;

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
