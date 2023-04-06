use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct InvalidTODOTag {
    pub tag: String,
}

impl Violation for InvalidTODOTag {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidTODOTag { tag } = self;
        format!("Invalid TODO tag {tag}: should be `TODO`")
    }
}
