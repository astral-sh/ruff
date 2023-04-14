use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::AsRule;

#[violation]
pub struct FirstLineCapitalized {
    pub first_word: String,
    pub capitalized_word: String,
}

impl AlwaysAutofixableViolation for FirstLineCapitalized {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "First word of the first line should be capitalized: `{}` -> `{}`",
            self.first_word, self.capitalized_word
        )
    }

    fn autofix_title(&self) -> String {
        format!(
            "Capitalize `{}` to `{}`",
            self.first_word, self.capitalized_word
        )
    }
}

/// D403
pub fn capitalized(checker: &mut Checker, docstring: &Docstring) {
    if !matches!(
        docstring.kind,
        DefinitionKind::Function(_) | DefinitionKind::NestedFunction(_) | DefinitionKind::Method(_)
    ) {
        return;
    }

    let body = docstring.body();

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
    let mut first_word_chars = first_word.chars();
    let Some(first_char) = first_word_chars.next() else {
        return;
    };
    if first_char.is_uppercase() {
        return;
    };

    let capitalized_word = first_char.to_uppercase().to_string() + first_word_chars.as_str();

    let mut diagnostic = Diagnostic::new(
        FirstLineCapitalized {
            first_word: first_word.to_string(),
            capitalized_word: capitalized_word.to_string(),
        },
        docstring.expr.range(),
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::range_replacement(
            capitalized_word,
            TextRange::at(body.start(), first_word.text_len()),
        ));
    }

    checker.diagnostics.push(diagnostic);
}
