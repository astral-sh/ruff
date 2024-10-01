use ruff_formatter::FormatContext;
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;

use crate::prelude::*;
use crate::MagicTrailingComma;

/// Returns `true` if the range ends with a magic trailing comma (and the magic trailing comma
/// should be respected).
pub(crate) fn has_magic_trailing_comma(range: TextRange, context: &PyFormatContext) -> bool {
    match context.options().magic_trailing_comma() {
        MagicTrailingComma::Respect => has_trailing_comma(range, context),
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
