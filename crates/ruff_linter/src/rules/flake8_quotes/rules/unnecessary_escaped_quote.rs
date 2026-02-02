use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, AnyStringFlags, InterpolatedStringElements, StringFlags, StringLike,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

use crate::rules::flake8_quotes::helpers::{contains_escaped_quote, raw_contents, unescape_string};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0")]
pub(crate) struct UnnecessaryEscapedQuote;

impl AlwaysFixableViolation for UnnecessaryEscapedQuote {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary escape on inner quote character".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove backslash".to_string()
    }
}

/// Q004
pub(crate) fn unnecessary_escaped_quote(checker: &Checker, string_like: StringLike) {
    if checker.semantic().in_pep_257_docstring() {
        return;
    }

    for part in string_like.parts() {
        match part {
            ast::StringLikePart::String(string_literal) => check_string_or_bytes(
                checker,
                string_literal.range(),
                AnyStringFlags::from(string_literal.flags),
            ),
            ast::StringLikePart::Bytes(bytes_literal) => check_string_or_bytes(
                checker,
                bytes_literal.range(),
                AnyStringFlags::from(bytes_literal.flags),
            ),
            ast::StringLikePart::FString(ast::FString {
                elements,
                range,
                node_index: _,
                flags,
            }) => {
                check_interpolated_string(checker, AnyStringFlags::from(*flags), *range, elements);
            }
            ast::StringLikePart::TString(ast::TString {
                elements,
                range,
                node_index: _,
                flags,
            }) => {
                check_interpolated_string(checker, AnyStringFlags::from(*flags), *range, elements);
            }
        }
    }
}

/// Checks for unnecessary escaped quotes in a string or bytes literal.
///
/// # Panics
///
/// If the string kind is an f-string.
fn check_string_or_bytes(checker: &Checker, range: TextRange, flags: AnyStringFlags) {
    assert!(!flags.is_interpolated_string());

    if flags.is_triple_quoted() || flags.is_raw_string() {
        return;
    }

    let contents = raw_contents(checker.locator().slice(range), flags);
    let quote = flags.quote_style();
    let opposite_quote_char = quote.opposite().as_char();

    if !contains_escaped_quote(contents, opposite_quote_char) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(UnnecessaryEscapedQuote, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        flags
            .display_contents(&unescape_string(contents, opposite_quote_char))
            .to_string(),
        range,
    )));
}

/// Checks for unnecessary escaped quotes in an f-string or t-string.
fn check_interpolated_string(
    checker: &Checker,
    flags: AnyStringFlags,
    range: TextRange,
    elements: &InterpolatedStringElements,
) {
    if flags.is_triple_quoted() || flags.prefix().is_raw() {
        return;
    }

    let opposite_quote_char = flags.quote_style().opposite().as_char();

    let mut edits = vec![];
    for literal in elements.literals() {
        let content = checker.locator().slice(literal);
        if !contains_escaped_quote(content, opposite_quote_char) {
            continue;
        }
        edits.push(Edit::range_replacement(
            unescape_string(content, opposite_quote_char),
            literal.range(),
        ));
    }

    let mut edits_iter = edits.into_iter();
    let Some(first) = edits_iter.next() else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(UnnecessaryEscapedQuote, range);
    diagnostic.set_fix(Fix::safe_edits(first, edits_iter));
}
