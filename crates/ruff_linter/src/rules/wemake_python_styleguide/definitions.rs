use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct TooManyModuleMembers {
    module_members: usize,
    max_module_members: usize,
}

impl Violation for TooManyModuleMembers {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyModuleMembers {
            module_members,
            max_module_members
        } = self;
        format!("Found a module with too many members ({module_members} > {max_module_members})")
    }
}
