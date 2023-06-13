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

pub(crate) fn todos(directive_ranges: &[TodoComment]) -> Vec<Diagnostic> {
    directive_ranges
        .iter()
        .map(|TodoComment { directive, .. }| match directive.kind {
            // FIX001
            TodoDirectiveKind::Fixme => Diagnostic::new(LineContainsFixme, directive.range),
            // FIX002
            TodoDirectiveKind::Hack => Diagnostic::new(LineContainsHack, directive.range),
            // FIX003
            TodoDirectiveKind::Todo => Diagnostic::new(LineContainsTodo, directive.range),
            // FIX004
            TodoDirectiveKind::Xxx => Diagnostic::new(LineContainsXxx, directive.range),
        })
        .collect::<Vec<Diagnostic>>()
}
