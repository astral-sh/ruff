use ruff_diagnostic::AlwaysFixableViolation;

#[violation]
pub struct WhitespaceAfterDecorator;

impl AlwaysFixableViolation for WhitespaceAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace after decorator")
    }

    fn fix_title(&self) -> String {
        "Remove whitespace after decorator".to_string()
    }
}
