use itertools::Itertools;
use ruff_text_size::TextRange;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

use crate::rules::flake8_implicit_str_concat::settings::Settings;

/// ## What it does
/// Checks for implicitly concatenated strings on a single line.
///
/// ## Why is this bad?
/// While it is valid Python syntax to concatenate multiple string or byte
/// literals implicitly (via whitespace delimiters), it is unnecessary and
/// negatively affects code readability.
///
/// In some cases, the implicit concatenation may also be unintentional, as
/// autoformatters are capable of introducing single-line implicit
/// concatenations when collapsing long lines.
///
/// ## Example
/// ```python
/// z = "The quick " "brown fox."
/// ```
///
/// Use instead:
/// ```python
/// z = "The quick brown fox."
/// ```
#[violation]
pub struct SingleLineImplicitStringConcatenation;

impl Violation for SingleLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals on one line")
    }
}

/// ## What it does
/// Checks for implicitly concatenated strings that span multiple lines.
///
/// ## Why is this bad?
/// For string literals that wrap across multiple lines, PEP 8 recommends
/// the use of implicit string concatenation within parentheses instead of
/// using a backslash for line continuation, as the former is more readable
/// than the latter.
///
/// By default, this rule will only trigger if the string literal is
/// concatenated via a backslash. To disallow implicit string concatenation
/// altogether, set the `flake8-implicit-str-concat.allow-multiline` option
/// to `false`.
///
/// ## Options
/// - `flake8-implicit-str-concat.allow-multiline`
///
/// ## Example
/// ```python
/// z = "The quick brown fox jumps over the lazy "\
///     "dog."
/// ```
///
/// Use instead:
/// ```python
/// z = (
///     "The quick brown fox jumps over the lazy "
///     "dog."
/// )
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#maximum-line-length)
#[violation]
pub struct MultiLineImplicitStringConcatenation;

impl Violation for MultiLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals over multiple lines")
    }
}

/// ISC001, ISC002
pub(crate) fn implicit(
    tokens: &[LexResult],
    settings: &Settings,
    locator: &Locator,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    for ((a_tok, a_range), (b_tok, b_range)) in tokens
        .iter()
        .flatten()
        .filter(|(tok, _)| {
            !matches!(tok, Tok::Comment(..))
                && (settings.allow_multiline || !matches!(tok, Tok::NonLogicalNewline))
        })
        .tuple_windows()
    {
        if matches!(a_tok, Tok::String { .. }) && matches!(b_tok, Tok::String { .. }) {
            if locator.contains_line_break(TextRange::new(a_range.end(), b_range.start())) {
                diagnostics.push(Diagnostic::new(
                    MultiLineImplicitStringConcatenation,
                    TextRange::new(a_range.start(), b_range.end()),
                ));
            } else {
                diagnostics.push(Diagnostic::new(
                    SingleLineImplicitStringConcatenation,
                    TextRange::new(a_range.start(), b_range.end()),
                ));
            }
        }
    }
    diagnostics
}
