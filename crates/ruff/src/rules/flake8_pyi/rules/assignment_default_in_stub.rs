use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct AssignmentDefaultInStub;
impl Violation for AssignmentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        todo!("implement message");
        format!("TODO: write message")
    }
}

/// PYI015
pub fn assignment_default_in_stub(checker: &mut Checker) {}
