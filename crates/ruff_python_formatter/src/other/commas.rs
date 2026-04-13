use ruff_formatter::FormatContext;
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;

use crate::MagicTrailingComma;
use crate::prelude::*;

/// Returns `true` if the range ends with a magic trailing comma that should be respected.
///
/// Returns `true` for `Respect` and `Force` when a trailing comma is present —
/// both modes treat trailing commas as expansion signals:
/// ```python
/// # Trailing comma keeps this expanded in Respect and Force:
/// result = func(
///     arg1,
///     arg2,
/// )
/// ```
/// With `Ignore`, trailing commas are never treated as expansion signals.
pub(crate) fn has_magic_trailing_comma(range: TextRange, context: &PyFormatContext) -> bool {
    match context.options().magic_trailing_comma() {
        MagicTrailingComma::Respect | MagicTrailingComma::Force => {
            has_trailing_comma(range, context)
        }
        MagicTrailingComma::Ignore => false,
    }
}

/// Returns `true` if the range ends with a trailing comma.
pub(crate) fn has_trailing_comma(range: TextRange, context: &PyFormatContext) -> bool {
    let first_token = SimpleTokenizer::new(context.source(), range)
        .skip_trivia()
        // Skip over any closing parentheses belonging to the expression
        .find(|token| token.kind() != SimpleTokenKind::RParen);

    matches!(
        first_token,
        Some(SimpleToken {
            kind: SimpleTokenKind::Comma,
            ..
        })
    )
}
