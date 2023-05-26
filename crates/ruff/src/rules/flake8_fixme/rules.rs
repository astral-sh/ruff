use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct LineContainsTodo;
impl Violation for LineContainsTodo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains TODO")
    }
}

#[violation]
pub struct LineContainsFixme;
impl Violation for LineContainsFixme {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains FIXME")
    }
}

#[violation]
pub struct LineContainsXxx;
impl Violation for LineContainsXxx {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains XXX")
    }
}

#[violation]
pub struct LineContainsHack;
impl Violation for LineContainsHack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Line contains HACK")
    }
}

pub fn todos(directive_ranges: Vec<(TodoDirective, TextRange)>) -> Vec<Diagnostic> {
    directive_ranges
        .into_iter()
        .map(|(directive, range)| match directive {
            TodoDirective::Fixme => Diagnostic::new(LineContainsFixme, range),
            TodoDirective::Hack => Diagnostic::new(LineContainsHack, range),
            TodoDirective::Todo => Diagnostic::new(LineContainsTodo, range),
            TodoDirective::Xxx => Diagnostic::new(LineContainsXxx, range),
        })
        .collect::<Vec<Diagnostic>>()
}
