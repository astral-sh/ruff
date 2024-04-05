use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

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

impl AlwaysFixableViolation for FirstLineCapitalized {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "First word of the first line should be capitalized: `{}` -> `{}`",
            self.first_word, self.capitalized_word
        )
    }

    fn fix_title(&self) -> String {
        format!(
            "Capitalize `{}` to `{}`",
            self.first_word, self.capitalized_word
        )
    }
}

/// D403
pub(crate) fn capitalized(checker: &mut Checker, docstring: &Docstring) {
    if docstring.definition.as_function_def().is_none() {
        return;
    }

    let body = docstring.body();
    let first_word = body.split_once(' ').map_or_else(
        || {
            // If the docstring is a single word, trim the punctuation marks because
            // it makes the ASCII test below fail.
            body.trim_end_matches(['.', '!', '?'])
        },
        |(first_word, _)| first_word,
    );

    let mut first_word_chars = first_word.chars();
    let Some(first_char) = first_word_chars.next() else {
        return;
    };

    if !first_char.is_ascii() {
        return;
    }

    let uppercase_first_char = first_char.to_ascii_uppercase();
    if first_char == uppercase_first_char {
        return;
    }

    // Like pydocstyle, we only support ASCII for now.
    for char in first_word.chars().skip(1) {
        if !char.is_ascii_alphabetic() && char != '\'' {
            return;
        }
    }

    let capitalized_word = uppercase_first_char.to_string() + first_word_chars.as_str();

    let mut diagnostic = Diagnostic::new(
        FirstLineCapitalized {
            first_word: first_word.to_string(),
            capitalized_word: capitalized_word.to_string(),
        },
        docstring.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        capitalized_word,
        TextRange::at(body.start(), first_word.text_len()),
    )));

    checker.diagnostics.push(diagnostic);
}
