use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::definition::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::AsRule;

#[violation]
pub struct FirstLineCapitalized {
    first_word: String,
    capitalized_word: String,
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
pub(crate) fn capitalized(checker: &mut Checker, docstring: &Docstring) {
    if !matches!(
        docstring.definition,
        Definition::Member(Member {
            kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
            ..
        })
    ) {
        return;
    }

    let body = docstring.body();
    let Some(first_word) = body.split(' ').next() else {
        return
    };

    // Like pydocstyle, we only support ASCII for now.
    for char in first_word.chars() {
        if !char.is_ascii_alphabetic() && char != '\'' {
            return;
        }
    }

    let mut first_word_chars = first_word.chars();
    let Some(first_char) = first_word_chars.next() else {
        return;
    };
    let uppercase_first_char = first_char.to_ascii_uppercase();
    if first_char == uppercase_first_char {
        return;
    }

    let capitalized_word = uppercase_first_char.to_string() + first_word_chars.as_str();

    let mut diagnostic = Diagnostic::new(
        FirstLineCapitalized {
            first_word: first_word.to_string(),
            capitalized_word: capitalized_word.to_string(),
        },
        docstring.expr.range(),
    );

    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            capitalized_word,
            TextRange::at(body.start(), first_word.text_len()),
        )));
    }

    checker.diagnostics.push(diagnostic);
}
