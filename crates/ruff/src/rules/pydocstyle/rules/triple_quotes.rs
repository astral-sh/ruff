use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for docstrings that use `'''triple single quotes'''` instead of
/// `"""triple double quotes"""`.
///
/// ## Why is this bad?
/// [PEP 257](https://peps.python.org/pep-0257/#what-is-a-docstring) recommends
/// the use of `"""triple double quotes"""` for docstrings, to ensure
/// consistency.
///
/// ## Example
/// ```python
/// def kos_root():
///     '''Return the pathname of the KOS root directory.'''
/// ```
///
/// Use instead:
/// ```python
/// def kos_root():
///     """Return the pathname of the KOS root directory."""
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
#[violation]
pub struct TripleSingleQuotes;

impl Violation for TripleSingleQuotes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use triple double quotes `"""`"#)
    }
}

/// D300
pub(crate) fn triple_quotes(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    let leading_quote = docstring.leading_quote().to_ascii_lowercase();

    let starts_with_triple = if body.contains("\"\"\"") {
        matches!(leading_quote.as_str(), "'''" | "u'''" | "r'''" | "ur'''")
    } else {
        matches!(
            leading_quote.as_str(),
            "\"\"\"" | "u\"\"\"" | "r\"\"\"" | "ur\"\"\""
        )
    };
    if !starts_with_triple {
        checker
            .diagnostics
            .push(Diagnostic::new(TripleSingleQuotes, docstring.range()));
    }
}
