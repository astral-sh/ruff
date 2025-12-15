use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for docstrings that do not start with a capital letter.
///
/// ## Why is this bad?
/// The first non-whitespace character in a docstring should be
/// capitalized for grammatical correctness and consistency.
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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.69")]
pub(crate) struct FirstWordUncapitalized {
    first_word: String,
    capitalized_word: String,
}

impl AlwaysFixableViolation for FirstWordUncapitalized {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "First word of the docstring should be capitalized: `{}` -> `{}`",
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
pub(crate) fn capitalized(checker: &Checker, docstring: &Docstring) {
    if docstring.definition.as_function_def().is_none() {
        return;
    }

    let body = docstring.body();
    let trim_start_body = body.trim_start();
    let first_word = trim_start_body
        .find(char::is_whitespace)
        .map_or(trim_start_body, |idx| &trim_start_body[..idx])
        .trim_end_matches(['.', '!', '?']);

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

    let leading_whitespace_len = body.text_len() - trim_start_body.text_len();

    let mut diagnostic = checker.report_diagnostic(
        FirstWordUncapitalized {
            first_word: first_word.to_string(),
            capitalized_word: capitalized_word.clone(),
        },
        docstring.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        capitalized_word,
        TextRange::at(body.start() + leading_whitespace_len, first_word.text_len()),
    )));
}
