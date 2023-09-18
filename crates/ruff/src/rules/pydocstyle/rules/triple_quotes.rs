use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Quote;
use ruff_text_size::Ranged;

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
pub struct TripleSingleQuotes {
    expected_quote: Quote,
}

impl Violation for TripleSingleQuotes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TripleSingleQuotes { expected_quote } = self;
        match expected_quote {
            Quote::Double => format!(r#"Use triple double quotes `"""`"#),
            Quote::Single => format!(r#"Use triple single quotes `'''`"#),
        }
    }
}

/// D300
pub(crate) fn triple_quotes(checker: &mut Checker, docstring: &Docstring) {
    let leading_quote = docstring.leading_quote();

    let expected_quote = if docstring.body().contains("\"\"\"") {
        Quote::Single
    } else {
        Quote::Double
    };

    match expected_quote {
        Quote::Single => {
            if !leading_quote.ends_with("'''") {
                checker.diagnostics.push(Diagnostic::new(
                    TripleSingleQuotes { expected_quote },
                    docstring.range(),
                ));
            }
        }
        Quote::Double => {
            if !leading_quote.ends_with("\"\"\"") {
                checker.diagnostics.push(Diagnostic::new(
                    TripleSingleQuotes { expected_quote },
                    docstring.range(),
                ));
            }
        }
    }
}
