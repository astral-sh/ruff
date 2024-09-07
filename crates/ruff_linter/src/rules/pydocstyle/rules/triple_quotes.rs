use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::Quote;
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent quotes, making the rule redundant.
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [formatter]: https://docs.astral.sh/ruff/formatter/
#[violation]
pub struct TripleSingleQuotes {
    expected_quote: Quote,
}

impl Violation for TripleSingleQuotes {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let TripleSingleQuotes { expected_quote } = self;
        match expected_quote {
            Quote::Double => format!(r#"Use triple double quotes `"""`"#),
            Quote::Single => format!(r"Use triple single quotes `'''`"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let TripleSingleQuotes { expected_quote } = self;
        Some(match expected_quote {
            Quote::Double => format!("Convert to triple double quotes"),
            Quote::Single => format!("Convert to triple single quotes"),
        })
    }
}

/// D300
pub(crate) fn triple_quotes(checker: &mut Checker, docstring: &Docstring) {
    let leading_quote = docstring.leading_quote();

    let prefixes = leading_quote.trim_end_matches(['\'', '"']).to_owned();

    let expected_quote = if docstring.body().contains("\"\"\"") {
        if docstring.body().contains("\'\'\'") {
            return;
        }
        Quote::Single
    } else {
        Quote::Double
    };

    match expected_quote {
        Quote::Single => {
            if !leading_quote.ends_with("'''") {
                let mut diagnostic =
                    Diagnostic::new(TripleSingleQuotes { expected_quote }, docstring.range());

                let body = docstring.body().as_str();
                if !body.ends_with('\'') {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        format!("{prefixes}'''{body}'''"),
                        docstring.range(),
                    )));
                }

                checker.diagnostics.push(diagnostic);
            }
        }
        Quote::Double => {
            if !leading_quote.ends_with("\"\"\"") {
                let mut diagnostic =
                    Diagnostic::new(TripleSingleQuotes { expected_quote }, docstring.range());

                let body = docstring.body().as_str();
                if !body.ends_with('"') {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        format!("{prefixes}\"\"\"{body}\"\"\""),
                        docstring.range(),
                    )));
                }

                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
