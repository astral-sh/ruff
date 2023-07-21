use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::AsRule;

/// ## What it does
/// Checks for docstrings that do not start with a capital letter.
///
/// ## Why is this bad?
/// The first character in a docstring should be capitalized for, grammatical
/// correctness and consistency.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """return the mean of the given values."""
/// ```
///
/// Use instead:
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
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
        return;
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
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            capitalized_word,
            TextRange::at(body.start(), first_word.text_len()),
        )));
    }

    checker.diagnostics.push(diagnostic);
}
