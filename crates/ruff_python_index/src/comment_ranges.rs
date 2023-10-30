use std::fmt::Debug;

use ruff_python_ast::PySourceType;
use ruff_python_parser::lexer::{lex, LexicalError};
use ruff_python_parser::{AsMode, Tok};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextRange;

#[derive(Debug, Clone, Default)]
pub struct CommentRangesBuilder {
    ranges: Vec<TextRange>,
}

impl CommentRangesBuilder {
    pub fn visit_token(&mut self, token: &Tok, range: TextRange) {
        if token.is_comment() {
            self.ranges.push(range);
        }
    }

    pub fn finish(self) -> CommentRanges {
        CommentRanges::new(self.ranges)
    }
}

/// Helper method to lex and extract comment ranges
pub fn tokens_and_ranges(
    source: &str,
    source_type: PySourceType,
) -> Result<(Vec<(Tok, TextRange)>, CommentRanges), LexicalError> {
    let mut tokens = Vec::new();
    let mut comment_ranges = CommentRangesBuilder::default();

    for result in lex(source, source_type.as_mode()) {
        let (token, range) = result?;

        comment_ranges.visit_token(&token, range);
        tokens.push((token, range));
    }

    let comment_ranges = comment_ranges.finish();
    Ok((tokens, comment_ranges))
}
