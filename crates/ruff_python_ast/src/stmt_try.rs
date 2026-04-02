use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::StmtTry;

/// Return the `Range` of the `Else` token in a `Try` statement.
pub fn try_else_range(stmt_try: &StmtTry, contents: &str) -> Option<TextRange> {
    let search_start = stmt_try.handlers.last().map(Ranged::end)?;

    let token = SimpleTokenizer::starts_at(search_start, contents)
        .skip_trivia()
        .next()?;

    matches!(token.kind, SimpleTokenKind::Else).then_some(token.range())
}
