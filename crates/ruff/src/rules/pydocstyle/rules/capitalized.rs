use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct FirstLineCapitalized;
);
impl Violation for FirstLineCapitalized {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First word of the first line should be properly capitalized")
    }
}

/// D403
pub fn capitalized(checker: &mut Checker, docstring: &Docstring) {
    if !matches!(docstring.kind, DefinitionKind::Function(_)) {
        return;
    }

    let body = docstring.body;

    let Some(first_word) = body.split(' ').next() else {
        return
    };
    if first_word == first_word.to_uppercase() {
        return;
    }
    for char in first_word.chars() {
        if !char.is_ascii_alphabetic() && char != '\'' {
            return;
        }
    }
    let Some(first_char) = first_word.chars().next() else {
        return;
    };
    if first_char.is_uppercase() {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        FirstLineCapitalized,
        Range::from_located(docstring.expr),
    ));
}
