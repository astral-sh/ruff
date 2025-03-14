use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_source_file::NewlineWithTrailingNewline;
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

use crate::rules::pydocstyle::helpers::ends_with_backslash;

/// ## What it does
/// Checks for surrounding whitespace in docstrings.
///
/// ## Why is this bad?
/// Remove surrounding whitespace from the docstring, for consistency.
///
/// ## Example
/// ```python
/// def factorial(n: int) -> int:
///     """ Return the factorial of n. """
/// ```
///
/// Use instead:
/// ```python
/// def factorial(n: int) -> int:
///     """Return the factorial of n."""
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[derive(ViolationMetadata)]
pub(crate) struct SurroundingWhitespace;

impl Violation for SurroundingWhitespace {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "No whitespaces allowed surrounding docstring text".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Trim surrounding whitespace".to_string())
    }
}

/// D210
pub(crate) fn no_surrounding_whitespace(checker: &Checker, docstring: &Docstring) {
    let body = docstring.body();

    let mut lines = NewlineWithTrailingNewline::from(body.as_str());
    let Some(line) = lines.next() else {
        return;
    };
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if line == trimmed {
        return;
    }
    let mut diagnostic = Diagnostic::new(SurroundingWhitespace, docstring.range());
    let quote = docstring.quote_style().as_char();
    // If removing whitespace would lead to an invalid string of quote
    // characters, avoid applying the fix.
    if !trimmed.ends_with(quote) && !trimmed.starts_with(quote) && !ends_with_backslash(trimmed) {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            trimmed.to_string(),
            TextRange::at(body.start(), line.text_len()),
        )));
    }
    checker.report_diagnostic(diagnostic);
}
