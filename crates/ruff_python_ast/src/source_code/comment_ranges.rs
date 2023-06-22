use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use ruff_text_size::TextRange;
use rustpython_parser::Tok;

/// Stores the ranges of comments sorted by [`TextRange::start`] in increasing order. No two ranges are overlapping.
#[derive(Clone)]
pub struct CommentRanges {
    raw: Vec<TextRange>,
}

impl Deref for CommentRanges {
    type Target = [TextRange];

    fn deref(&self) -> &Self::Target {
        self.raw.as_slice()
    }
}

impl Debug for CommentRanges {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CommentRanges").field(&self.raw).finish()
    }
}

impl<'a> IntoIterator for &'a CommentRanges {
    type IntoIter = std::slice::Iter<'a, TextRange>;
    type Item = &'a TextRange;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter()
    }
}

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
        CommentRanges { raw: self.ranges }
    }
}
