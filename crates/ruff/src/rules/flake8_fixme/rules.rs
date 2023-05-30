use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::directives::{TodoComment, TodoDirectiveKind};

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

pub fn todos(directive_ranges: &[TodoComment]) -> Vec<Diagnostic> {
    directive_ranges
        .iter()
        .map(|TodoComment { directive, .. }| match directive.kind {
            // T-001
            TodoDirectiveKind::Fixme => Diagnostic::new(LineContainsFixme, directive.range),
            // T-002
            TodoDirectiveKind::Hack => Diagnostic::new(LineContainsHack, directive.range),
            // T-003
            TodoDirectiveKind::Todo => Diagnostic::new(LineContainsTodo, directive.range),
            // T-004
            TodoDirectiveKind::Xxx => Diagnostic::new(LineContainsXxx, directive.range),
        })
        .collect::<Vec<Diagnostic>>()
}
