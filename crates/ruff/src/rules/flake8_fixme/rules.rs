use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct LineContainsTodo {}
impl Violation for LineContainsTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains TODO")
    }
}

#[violation]
pub struct LineContainsFixme {}
impl Violation for LineContainsFixme {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains FIXME")
    }
}

#[violation]
pub struct LineContainsXxx {}
impl Violation for LineContainsXxx {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains XXX")
    }
}

#[violation]
pub struct LineContainsHack {}
impl Violation for LineContainsHack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains HACK")
    }
}
