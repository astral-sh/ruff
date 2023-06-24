use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for docstrings that
///
/// ## Why is this bad?
/// Backslashes are used to escape characters in strings, unless the string is
/// defined as a raw string literal. Use the raw string literal prefix `r` when
/// declaring docstrings with backslashes to avoid confusion and accidental
/// escaping.
///
/// ## Example
/// ```python
/// def foobar():
///     """Docstring for foo\bar."""
///
///
/// foobar.__doc__  # "Docstring for foar."
/// ```
///
/// Use instead:
/// ```python
/// def foobar():
///     r"""Docstring for foo\bar."""
///
///
/// foobar.__doc__  # "Docstring for foo\bar."
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [Python documentation: String and Bytes literals](https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals)
#[violation]
pub struct EscapeSequenceInDocstring;

impl Violation for EscapeSequenceInDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use `r"""` if any backslashes in a docstring"#)
    }
}

static BACKSLASH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\[^(\r\n|\n)uN]").unwrap());

/// D301
pub(crate) fn backslashes(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;

    // Docstring is already raw.
    if contents.starts_with('r') || contents.starts_with("ur") {
        return;
    }

    if BACKSLASH_REGEX.is_match(contents) {
        checker.diagnostics.push(Diagnostic::new(
            EscapeSequenceInDocstring,
            docstring.range(),
        ));
    }
}
