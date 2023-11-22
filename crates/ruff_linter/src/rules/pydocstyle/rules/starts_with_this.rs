use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::helpers::normalize_word;

/// ## What it does
/// Checks for docstrings that start with `This`.
///
/// ## Why is this bad?
/// [PEP 257] recommends that the first line of a docstring be written in the
/// imperative mood, for consistency.
///
/// Hint: to rewrite the docstring in the imperative, phrase the first line as
/// if it were a command.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `numpy`
/// convention,, and disabled when using the `google` and `pep257` conventions.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """This function returns the mean of the given values."""
/// ```
///
/// Use instead:
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
/// ```
///
/// ## Options
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct DocstringStartsWithThis;

impl Violation for DocstringStartsWithThis {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"First word of the docstring should not be "This""#)
    }
}

/// D404
pub(crate) fn starts_with_this(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }

    let Some(first_word) = trimmed.split(' ').next() else {
        return;
    };
    if normalize_word(first_word) != "this" {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(DocstringStartsWithThis, docstring.range()));
}
