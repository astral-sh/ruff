use ruff_python_parser::{Token, TokenKind};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::{Ranged, TextRange};

#[derive(Debug, Clone, Default)]
pub(crate) struct CommentRangesBuilder {
    ranges: Vec<TextRange>,
}

impl CommentRangesBuilder {
    pub(crate) fn visit_token(&mut self, token: &Token) {
        if token.kind() == TokenKind::Comment {
            self.ranges.push(token.range());
        }
    }

    pub(crate) fn finish(self) -> CommentRanges {
        CommentRanges::new(self.ranges)
    }
}
