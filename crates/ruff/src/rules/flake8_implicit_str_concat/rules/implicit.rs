use itertools::Itertools;
use ruff_text_size::TextRange;
use rustpython_parser::Tok;
use rustpython_parser::{lexer::LexResult, StringKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
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
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals on one line")
    }

    fn autofix_title(&self) -> Option<String> {
        Option::Some("Combine these string literals into one".to_string())
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
                let mut diagnostic = Diagnostic::new(
                    SingleLineImplicitStringConcatenation,
                    TextRange::new(a_range.start(), b_range.end()),
                );

                if let Some(fix) = get_fix_for_single_line_implicit_string_concatenation(
                    &a_tok, *a_range, &b_tok, *b_range, locator,
                ) {
                    diagnostic.set_fix(fix);
                }

                diagnostics.push(diagnostic);
            };
        };
    }
    diagnostics
}

fn get_fix_for_single_line_implicit_string_concatenation(
    a_tok: &Tok,
    a_range: TextRange,
    b_tok: &Tok,
    b_range: TextRange,
    locator: &Locator,
) -> Option<Fix> {
    let (
        Tok::String {
            kind: a_kind,
            triple_quoted: a_triple_quoted,
            ..
        },
        Tok::String {
            kind: b_kind,
            triple_quoted: b_triple_quoted,
            ..
        },
    ) = (a_tok, b_tok) else { return Option::None };

    // Fix only strings of the same kind and triple-quotedness
    if a_kind != b_kind || a_triple_quoted != b_triple_quoted {
        return Option::None;
    }

    let a_text = &locator.contents()[a_range];
    let b_text = &locator.contents()[b_range];

    let a_quotes_style = get_quotes_style(a_text);
    let b_quotes_style = get_quotes_style(b_text);

    // Fix only strings with the same quotes style
    if a_quotes_style != b_quotes_style {
        return Option::None;
    }

    let text = skip_string_end(*a_triple_quoted, a_text).to_string()
        + skip_string_start(*b_kind, *b_triple_quoted, b_text);

    Option::Some(Fix::automatic(Edit::range_replacement(
        text,
        TextRange::new(a_range.start(), b_range.end()),
    )))
}

fn skip_string_start(kind: StringKind, triple_quoted: bool, text: &str) -> &str {
    let quotes_len = match kind {
        StringKind::String => 0,
        StringKind::Bytes | StringKind::FString | StringKind::Unicode | StringKind::RawString => 1,
        StringKind::RawBytes | StringKind::RawFString => 2,
    } + (if triple_quoted { 3 } else { 1 });

    &text[quotes_len..]
}

fn skip_string_end(triple_quoted: bool, text: &str) -> &str {
    let quotes_len = if triple_quoted { 3 } else { 1 };

    &text[..text.len() - quotes_len]
}

#[derive(PartialEq)]
enum QuotesStyle {
    Double,
    Single,
}

fn get_quotes_style(text: &str) -> QuotesStyle {
    if text.ends_with('\"') {
        QuotesStyle::Double
    } else {
        QuotesStyle::Single
    }
}
