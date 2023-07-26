use itertools::Itertools;
use ruff_text_size::TextRange;
use rustpython_parser::ast::Ranged;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for string literals that are explicitly concatenated (using the
/// `+` operator).
///
/// ## Why is this bad?
/// For string literals that wrap across multiple lines, implicit string
/// concatenation within parentheses is preferred over explicit
/// concatenation using the `+` operator, as the former is more readable.
///
/// ## Example
/// ```python
/// z = (
///     "The quick brown fox jumps over the lazy "
///     + "dog"
/// )
/// ```
///
/// Use instead:
/// ```python
/// z = (
///     "The quick brown fox jumps over the lazy "
///     "dog"
/// )
/// ```
#[violation]
pub struct ExplicitStringConcatenation;

impl Violation for ExplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Explicitly concatenated string should be implicitly concatenated")
    }
}

/// ISC003
pub(crate) fn explicit(diagnostics: &mut Vec<Diagnostic>, tokens: &[LexResult]) {
    for ((a_tok, a_range), (b_tok, _), (c_tok, _), (d_tok, d_range)) in tokens
        .iter()
        .flatten()
        .filter(|(tok, _)| !tok.is_comment())
        .tuple_windows()
    {
        if matches!(
            (a_tok, b_tok, c_tok, d_tok),
            (
                Tok::String { .. },
                Tok::NonLogicalNewline,
                Tok::Plus,
                Tok::String { .. }
            ) | (
                Tok::String { .. },
                Tok::Plus,
                Tok::NonLogicalNewline,
                Tok::String { .. }
            )
        ) {
            diagnostics.push(Diagnostic::new(
                ExplicitStringConcatenation,
                TextRange::new(a_range.start(), d_range.end()),
            ));
        }
    }
}
