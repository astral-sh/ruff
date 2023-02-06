use crate::define_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;

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
