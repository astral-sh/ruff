use crate::define_violation;

use crate::violation::Violation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct RedefinedWhileUnused {
        pub name: String,
        pub line: usize,
    }
);
impl Violation for RedefinedWhileUnused {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedWhileUnused { name, line } = self;
        format!("Redefinition of unused `{name}` from line {line}")
    }
}
