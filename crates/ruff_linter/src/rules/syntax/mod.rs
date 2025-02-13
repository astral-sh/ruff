use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

#[derive(ViolationMetadata)]
pub struct VersionSyntaxError {
    pub message: String,
}

impl Violation for VersionSyntaxError {
    #[allow(clippy::useless_format)]
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("{}", self.message)
    }
}
