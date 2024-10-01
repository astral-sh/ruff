use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, AnyStringFlags, StringFlags, StringLike};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

use super::super::helpers::{contains_escaped_quote, raw_contents, unescape_string};

/// ## What it does
/// Checks for strings that include unnecessarily escaped quotes.
///
/// ## Why is this bad?
/// If a string contains an escaped quote that doesn't match the quote
/// character used for the string, it's unnecessary and can be removed.
///
/// ## Example
/// ```python
/// foo = "bar\'s"
/// ```
///
/// Use instead:
/// ```python
/// foo = "bar's"
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter automatically removes unnecessary escapes, making the rule
/// redundant.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[violation]
pub struct UnnecessaryEscapedQuote;

impl AlwaysFixableViolation for UnnecessaryEscapedQuote {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary escape on inner quote character")
    }

    fn fix_title(&self) -> String {
        "Remove backslash".to_string()
    }
}

/// Q004
pub(crate) fn unnecessary_escaped_quote(checker: &mut Checker, string_like: StringLike) {
    if checker.semantic().in_pep_257_docstring() {
        return;
    }

    let locator = checker.locator();

    for part in string_like.parts() {
        match part {
            ast::StringLikePart::String(string_literal) => {
                if let Some(diagnostic) = check_string_or_bytes(
                    locator,
                    string_literal.range(),
                    AnyStringFlags::from(string_literal.flags),
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            ast::StringLikePart::Bytes(bytes_literal) => {
                if let Some(diagnostic) = check_string_or_bytes(
                    locator,
                    bytes_literal.range(),
                    AnyStringFlags::from(bytes_literal.flags),
                ) {
                    checker.diagnostics.push(diagnostic);
                }
            }
            ast::StringLikePart::FString(f_string) => {
                if let Some(diagnostic) = check_f_string(locator, f_string) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// Checks for unnecessary escaped quotes in a string or bytes literal.
///
/// # Panics
///
/// If the string kind is an f-string.
fn check_string_or_bytes(
    locator: &Locator,
    range: TextRange,
    flags: AnyStringFlags,
) -> Option<Diagnostic> {
    assert!(!flags.is_f_string());

    if flags.is_triple_quoted() || flags.is_raw_string() {
        return None;
    }

    let contents = raw_contents(locator.slice(range), flags);
    let quote = flags.quote_style();
    let opposite_quote_char = quote.opposite().as_char();

    if !contains_escaped_quote(contents, opposite_quote_char) {
        return None;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryEscapedQuote, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        flags.format_string_contents(&unescape_string(contents, opposite_quote_char)),
        range,
    )));
    Some(diagnostic)
}

/// Checks for unnecessary escaped quotes in an f-string.
fn check_f_string(locator: &Locator, f_string: &ast::FString) -> Option<Diagnostic> {
    let ast::FString { flags, range, .. } = f_string;
    if flags.is_triple_quoted() || flags.prefix().is_raw() {
        return None;
    }

    let opposite_quote_char = flags.quote_style().opposite().as_char();

    let mut edits = vec![];
    for literal in f_string.elements.literals() {
        let content = locator.slice(literal);
        if !contains_escaped_quote(content, opposite_quote_char) {
            continue;
        }
        edits.push(Edit::range_replacement(
            unescape_string(content, opposite_quote_char),
            literal.range(),
        ));
    }

    let mut edits_iter = edits.into_iter();
    let first = edits_iter.next()?;

    let mut diagnostic = Diagnostic::new(UnnecessaryEscapedQuote, *range);
    diagnostic.set_fix(Fix::safe_edits(first, edits_iter));
    Some(diagnostic)
}
