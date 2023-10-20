use itertools::Itertools;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_index::Indexer;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::settings::LinterSettings;

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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals on one line")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Combine string literals".to_string())
    }
}

/// ## What it does
/// Checks for implicitly concatenated strings that span multiple lines.
///
/// ## Why is this bad?
/// For string literals that wrap across multiple lines, [PEP 8] recommends
/// the use of implicit string concatenation within parentheses instead of
/// using a backslash for line continuation, as the former is more readable
/// than the latter.
///
/// By default, this rule will only trigger if the string literal is
/// concatenated via a backslash. To disallow implicit string concatenation
/// altogether, set the [`flake8-implicit-str-concat.allow-multiline`] option
/// to `false`.
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
/// ## Options
/// - `flake8-implicit-str-concat.allow-multiline`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length
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
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &[LexResult],
    settings: &LinterSettings,
    locator: &Locator,
    indexer: &Indexer,
) {
    for ((a_tok, a_range), (b_tok, b_range)) in tokens
        .iter()
        .flatten()
        .filter(|(tok, _)| {
            !tok.is_comment()
                && (settings.flake8_implicit_str_concat.allow_multiline
                    || !tok.is_non_logical_newline())
        })
        .tuple_windows()
    {
        let (a_range, b_range) = match (a_tok, b_tok) {
            (Tok::String { .. }, Tok::String { .. }) => (*a_range, *b_range),
            (Tok::String { .. }, Tok::FStringStart) => (
                *a_range,
                indexer.fstring_ranges().innermost(b_range.start()).unwrap(),
            ),
            (Tok::FStringEnd, Tok::String { .. }) => (
                indexer.fstring_ranges().innermost(a_range.start()).unwrap(),
                *b_range,
            ),
            (Tok::FStringEnd, Tok::FStringStart) => (
                indexer.fstring_ranges().innermost(a_range.start()).unwrap(),
                indexer.fstring_ranges().innermost(b_range.start()).unwrap(),
            ),
            _ => continue,
        };

        if locator.contains_line_break(TextRange::new(a_range.end(), b_range.start())) {
            diagnostics.push(Diagnostic::new(
                MultiLineImplicitStringConcatenation,
                TextRange::new(a_range.start(), b_range.end()),
            ));
        } else {
            let mut diagnostic = Diagnostic::new(
                SingleLineImplicitStringConcatenation,
                TextRange::new(a_range.start(), b_range.end()),
            );

            if let Some(fix) = concatenate_strings(a_range, b_range, locator) {
                diagnostic.set_fix(fix);
            }

            diagnostics.push(diagnostic);
        };
    }
}

fn concatenate_strings(a_range: TextRange, b_range: TextRange, locator: &Locator) -> Option<Fix> {
    let a_text = locator.slice(a_range);
    let b_text = locator.slice(b_range);

    let a_leading_quote = leading_quote(a_text)?;
    let b_leading_quote = leading_quote(b_text)?;

    // Require, for now, that the leading quotes are the same.
    if a_leading_quote != b_leading_quote {
        return None;
    }

    let a_trailing_quote = trailing_quote(a_text)?;
    let b_trailing_quote = trailing_quote(b_text)?;

    // Require, for now, that the trailing quotes are the same.
    if a_trailing_quote != b_trailing_quote {
        return None;
    }

    let a_body = &a_text[a_leading_quote.len()..a_text.len() - a_trailing_quote.len()];
    let b_body = &b_text[b_leading_quote.len()..b_text.len() - b_trailing_quote.len()];

    let concatenation = format!("{a_leading_quote}{a_body}{b_body}{a_trailing_quote}");
    let range = TextRange::new(a_range.start(), b_range.end());

    Some(Fix::safe_edit(Edit::range_replacement(
        concatenation,
        range,
    )))
}
