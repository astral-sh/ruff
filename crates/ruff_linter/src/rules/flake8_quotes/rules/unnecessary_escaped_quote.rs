use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::raw_contents;
use ruff_python_ast::{self as ast, StringLike};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::{contains_escaped_quote, unescape_string};

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
    let locator = checker.locator();

    match string_like {
        StringLike::String(expr) => {
            for string in &expr.value {
                if let Some(diagnostic) = check_string(locator, string) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        StringLike::Bytes(expr) => {
            for bytes in &expr.value {
                if let Some(diagnostic) = check_bytes(locator, bytes) {
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
        StringLike::FString(expr) => {
            for part in &expr.value {
                if let Some(diagnostic) = match part {
                    ast::FStringPart::Literal(string) => check_string(locator, string),
                    ast::FStringPart::FString(f_string) => check_f_string(locator, f_string),
                } {
                    checker.diagnostics.push(diagnostic);
                };
            }
        }
    }
}

fn check_string(locator: &Locator, string: &ast::StringLiteral) -> Option<Diagnostic> {
    let ast::StringLiteral { range, flags, .. } = string;
    if flags.is_triple_quoted() || flags.prefix().is_raw() {
        return None;
    }

    let contents = raw_contents(locator.slice(string))?;
    let quote = flags.quote_style();
    let opposite_quote_char = quote.opposite().as_char();

    if !contains_escaped_quote(contents, opposite_quote_char) {
        return None;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryEscapedQuote, *range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!(
            "{prefix}{quote}{value}{quote}",
            prefix = flags.prefix(),
            value = unescape_string(contents, opposite_quote_char)
        ),
        *range,
    )));
    Some(diagnostic)
}

fn check_bytes(locator: &Locator, bytes: &ast::BytesLiteral) -> Option<Diagnostic> {
    let ast::BytesLiteral { range, flags, .. } = bytes;
    if flags.is_triple_quoted() || flags.prefix().is_raw() {
        return None;
    }

    let contents = raw_contents(locator.slice(bytes))?;
    let quote = flags.quote_style();
    let opposite_quote_char = quote.opposite().as_char();

    if !contains_escaped_quote(contents, opposite_quote_char) {
        return None;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryEscapedQuote, *range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!(
            "{prefix}{quote}{value}{quote}",
            prefix = flags.prefix(),
            value = unescape_string(contents, opposite_quote_char)
        ),
        *range,
    )));
    Some(diagnostic)
}

fn check_f_string(locator: &Locator, f_string: &ast::FString) -> Option<Diagnostic> {
    let ast::FString { flags, range, .. } = f_string;
    if flags.is_triple_quoted() || flags.prefix().is_raw() {
        return None;
    }

    let opposite_quote_char = flags.quote_style().opposite().as_char();

    let mut edits = vec![];
    for literal in f_string.literals() {
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
