use std::borrow::Cow;

use itertools::Itertools;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StringFlags;
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_python_index::Indexer;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::{Edit, Fix, FixAvailability, Violation};

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
#[violation_metadata(stable_since = "v0.0.201")]
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
#[violation_metadata(stable_since = "v0.0.201")]
pub(crate) struct MultiLineImplicitStringConcatenation;

impl Violation for MultiLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Implicitly concatenated string literals over multiple lines".to_string()
    }
}

/// ISC001, ISC002
pub(crate) fn implicit(
    context: &LintContext,
    tokens: &Tokens,
    locator: &Locator,
    indexer: &Indexer,
) {
    for (a_token, b_token) in tokens
        .iter()
        .filter(|token| {
            token.kind() != TokenKind::Comment
                && (context
                    .settings()
                    .flake8_implicit_str_concat
                    .allow_multiline
                    || token.kind() != TokenKind::NonLogicalNewline)
        })
        .tuple_windows()
    {
        let (a_range, b_range) = match (a_token.kind(), b_token.kind()) {
            (TokenKind::String, TokenKind::String) => (a_token.range(), b_token.range()),
            (TokenKind::String, TokenKind::FStringStart) => {
                match indexer
                    .interpolated_string_ranges()
                    .innermost(b_token.start())
                {
                    Some(b_range) => (a_token.range(), b_range),
                    None => continue,
                }
            }
            (TokenKind::FStringEnd, TokenKind::String) => {
                match indexer
                    .interpolated_string_ranges()
                    .innermost(a_token.start())
                {
                    Some(a_range) => (a_range, b_token.range()),
                    None => continue,
                }
            }
            (TokenKind::FStringEnd, TokenKind::FStringStart)
            | (TokenKind::TStringEnd, TokenKind::TStringStart) => {
                match (
                    indexer
                        .interpolated_string_ranges()
                        .innermost(a_token.start()),
                    indexer
                        .interpolated_string_ranges()
                        .innermost(b_token.start()),
                ) {
                    (Some(a_range), Some(b_range)) => (a_range, b_range),
                    _ => continue,
                }
            }
            _ => continue,
        };

        if locator.contains_line_break(TextRange::new(a_range.end(), b_range.start())) {
            context.report_diagnostic_if_enabled(
                MultiLineImplicitStringConcatenation,
                TextRange::new(a_range.start(), b_range.end()),
            );
        } else {
            if let Some(mut diagnostic) = context.report_diagnostic_if_enabled(
                SingleLineImplicitStringConcatenation,
                TextRange::new(a_range.start(), b_range.end()),
            ) {
                if let Some(fix) = concatenate_strings(a_token, b_token, a_range, b_range, locator)
                {
                    diagnostic.set_fix(fix);
                }
            }
        }
    }
}

/// Concatenates two strings
///
/// The `a_string_range` and `b_string_range` are the range of the entire string,
/// not just of the string token itself (important for interpolated strings where
/// the start token doesn't span the entire token).
fn concatenate_strings(
    a_token: &Token,
    b_token: &Token,
    a_string_range: TextRange,
    b_string_range: TextRange,
    locator: &Locator,
) -> Option<Fix> {
    if a_token.string_flags()?.is_unclosed() || b_token.string_flags()?.is_unclosed() {
        return None;
    }

    let a_string_flags = a_token.string_flags()?;
    let b_string_flags = b_token.string_flags()?;

    let a_prefix = a_string_flags.prefix();
    let b_prefix = b_string_flags.prefix();

    // Require, for now, that the strings have the same prefix,
    // quote style, and number of quotes
    if a_prefix != b_prefix
        || a_string_flags.quote_style() != b_string_flags.quote_style()
        || a_string_flags.is_triple_quoted() != b_string_flags.is_triple_quoted()
    {
        return None;
    }

    let a_text = locator.slice(a_string_range);
    let b_text = locator.slice(b_string_range);

    let quotes = a_string_flags.quote_str();

    let opener_len = a_string_flags.opener_len();
    let closer_len = a_string_flags.closer_len();

    let mut a_body =
        Cow::Borrowed(&a_text[TextRange::new(opener_len, a_text.text_len() - closer_len)]);
    let b_body = &b_text[TextRange::new(opener_len, b_text.text_len() - closer_len)];

    if !a_string_flags.is_raw_string() && matches!(b_body.bytes().next(), Some(b'0'..=b'7')) {
        normalize_ending_octal(&mut a_body);
    }

    let concatenation = format!("{a_prefix}{quotes}{a_body}{b_body}{quotes}");
    let range = TextRange::new(a_string_range.start(), b_string_range.end());

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
