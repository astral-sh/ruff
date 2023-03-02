use ruff_macros::{derive_message_formats, violation};

use crate::violation::Violation;

#[violation]
pub struct ForwardAnnotationSyntaxError {
    pub body: String,
}

impl Violation for ForwardAnnotationSyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ForwardAnnotationSyntaxError { body } = self;
        format!("Syntax error in forward annotation: `{body}`")
    }
}
