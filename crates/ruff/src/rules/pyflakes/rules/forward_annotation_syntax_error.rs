use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

define_violation!(
    pub struct ForwardAnnotationSyntaxError {
        pub body: String,
    }
);
impl Violation for ForwardAnnotationSyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ForwardAnnotationSyntaxError { body } = self;
        format!("Syntax error in forward annotation: `{body}`")
    }
}
