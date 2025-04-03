use std::borrow::Cow;

use itertools::Itertools;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_index::Indexer;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::settings::LinterSettings;
use crate::Locator;

/// ## What it does
/// Checks for implicitly concatenated strings on a single line.
///
/// ## Why is this bad?
/// While it is valid Python syntax to concatenate multiple string or byte
/// literals implicitly (via whitespace delimiters), it is unnecessary and
/// negatively affects code readability.
///
/// In some cases, the implicit concatenation may also be unintentional, as
/// code formatters are capable of introducing single-line implicit
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
#[derive(ViolationMetadata)]
pub(crate) struct SingleLineImplicitStringConcatenation;

impl Violation for SingleLineImplicitStringConcatenation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Implicitly concatenated string literals on one line".to_string()
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
/// altogether, set the [`lint.flake8-implicit-str-concat.allow-multiline`] option
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
/// - `lint.flake8-implicit-str-concat.allow-multiline`
///
/// ## Formatter compatibility
/// Using this rule with `allow-multiline = false` can be incompatible with the
/// formatter because the [formatter] can introduce new multi-line implicitly
/// concatenated strings. We recommend to either:
///
/// * Enable `ISC001` to disallow all implicit concatenated strings
/// * Setting `allow-multiline = true`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[derive(ViolationMetadata)]
pub(crate) struct MultiLineImplicitStringConcatenation;

impl Violation for MultiLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Implicitly concatenated string literals over multiple lines".to_string()
    }
}

/// ISC001, ISC002
pub(crate) fn implicit(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &Tokens,
    locator: &Locator,
    indexer: &Indexer,
    settings: &LinterSettings,
) {
    for (a_token, b_token) in tokens
        .iter()
        .filter(|token| {
            token.kind() != TokenKind::Comment
                && (settings.flake8_implicit_str_concat.allow_multiline
                    || token.kind() != TokenKind::NonLogicalNewline)
        })
        .tuple_windows()
    {
        let (a_range, b_range) = match (a_token.kind(), b_token.kind()) {
            (TokenKind::String, TokenKind::String) => (a_token.range(), b_token.range()),
            (TokenKind::String, TokenKind::FStringStart) => {
                match indexer.fstring_ranges().innermost(b_token.start()) {
                    Some(b_range) => (a_token.range(), b_range),
                    None => continue,
                }
            }
            (TokenKind::FStringEnd, TokenKind::String) => {
                match indexer.fstring_ranges().innermost(a_token.start()) {
                    Some(a_range) => (a_range, b_token.range()),
                    None => continue,
                }
            }
            (TokenKind::FStringEnd, TokenKind::FStringStart) => {
                match (
                    indexer.fstring_ranges().innermost(a_token.start()),
                    indexer.fstring_ranges().innermost(b_token.start()),
                ) {
                    (Some(a_range), Some(b_range)) => (a_range, b_range),
                    _ => continue,
                }
            }
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
        }
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

    let mut a_body =
        Cow::Borrowed(&a_text[a_leading_quote.len()..a_text.len() - a_trailing_quote.len()]);
    let b_body = &b_text[b_leading_quote.len()..b_text.len() - b_trailing_quote.len()];

    if a_leading_quote.find(['r', 'R']).is_none()
        && matches!(b_body.bytes().next(), Some(b'0'..=b'7'))
    {
        normalize_ending_octal(&mut a_body);
    }

    let concatenation = format!("{a_leading_quote}{a_body}{b_body}{a_trailing_quote}");
    let range = TextRange::new(a_range.start(), b_range.end());

    Some(Fix::safe_edit(Edit::range_replacement(
        concatenation,
        range,
    )))
}

/// Pads an octal at the end of the string
/// to three digits, if necessary.
fn normalize_ending_octal(text: &mut Cow<'_, str>) {
    // Early return for short strings
    if text.len() < 2 {
        return;
    }

    let mut rev_bytes = text.bytes().rev();
    if let Some(last_byte @ b'0'..=b'7') = rev_bytes.next() {
        // "\y" -> "\00y"
        if has_odd_consecutive_backslashes(&mut rev_bytes.clone()) {
            let prefix = &text[..text.len() - 2];
            *text = Cow::Owned(format!("{prefix}\\00{}", last_byte as char));
        }
        // "\xy" -> "\0xy"
        else if let Some(penultimate_byte @ b'0'..=b'7') = rev_bytes.next() {
            if has_odd_consecutive_backslashes(&mut rev_bytes.clone()) {
                let prefix = &text[..text.len() - 3];
                *text = Cow::Owned(format!(
                    "{prefix}\\0{}{}",
                    penultimate_byte as char, last_byte as char
                ));
            }
        }
    }
}

fn has_odd_consecutive_backslashes(mut itr: impl Iterator<Item = u8>) -> bool {
    let mut odd_backslashes = false;
    while let Some(b'\\') = itr.next() {
        odd_backslashes = !odd_backslashes;
    }
    odd_backslashes
}
