use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for function docstrings that include the function's signature in
/// the summary line.
///
/// ## Why is this bad?
/// [PEP 257] recommends against including a function's signature in its
/// docstring. Instead, consider using type annotations as a form of
/// documentation for the function's parameters and return value.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `google` and
/// `pep257` conventions, and disabled when using the `numpy` convention.
///
/// ## Example
/// ```python
/// def foo(a, b):
///     """foo(a: int, b: int) -> list[int]"""
/// ```
///
/// Use instead:
/// ```python
/// def foo(a: int, b: int) -> list[int]:
///     """Return a list of a and b."""
/// ```
///
/// ## Options
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct NoSignature;

impl Violation for NoSignature {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should not be the function's signature")
    }
}

/// D402
pub(crate) fn no_signature(checker: &mut Checker, docstring: &Docstring) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };

    let body = docstring.body();

    let Some(first_line) = body.trim().universal_newlines().next() else {
        return;
    };

    // Search for occurrences of the function name followed by an open parenthesis (e.g., `foo(` for
    // a function named `foo`).
    if first_line
        .match_indices(function.name.as_str())
        .any(|(index, _)| first_line[index + function.name.len()..].starts_with('('))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(NoSignature, docstring.range()));
    }
}
