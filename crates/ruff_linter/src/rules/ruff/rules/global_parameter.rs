use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

#[derive(ViolationMetadata)]
pub(crate) struct GlobalParameter {
    pub(crate) name: String,
}

impl Violation for GlobalParameter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalParameter { name } = self;
        format!("name `{name}` is parameter and global")
    }
}
