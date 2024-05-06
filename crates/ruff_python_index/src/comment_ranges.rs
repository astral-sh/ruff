use std::fmt::Debug;

use ruff_python_ast::PySourceType;
use ruff_python_parser::lexer::{lex, LexResult, LexicalError};
use ruff_python_parser::{allocate_tokens_vec, AsMode, Tok};
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
) -> Result<(Vec<LexResult>, CommentRanges), LexicalError> {
    let mut tokens = allocate_tokens_vec(source);
    let mut comment_ranges = CommentRangesBuilder::default();

    for result in lex(source, source_type.as_mode()) {
        if let Ok((token, range)) = &result {
            comment_ranges.visit_token(token, *range);
        }

        tokens.push(result);
    }

    let comment_ranges = comment_ranges.finish();
    Ok((tokens, comment_ranges))
}
