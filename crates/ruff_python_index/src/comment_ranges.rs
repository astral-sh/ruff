use itertools::Itertools;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use ruff_python_parser::Tok;
use ruff_text_size::{Ranged, TextRange};

/// Stores the ranges of comments sorted by [`TextRange::start`] in increasing order. No two ranges are overlapping.
#[derive(Clone)]
pub struct CommentRanges {
    raw: Vec<TextRange>,
}

impl CommentRanges {
    /// Returns `true` if the given range includes a comment.
    pub fn intersects(&self, target: TextRange) -> bool {
        self.raw
            .binary_search_by(|range| {
                if target.contains_range(*range) {
                    std::cmp::Ordering::Equal
                } else if range.end() < target.start() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .is_ok()
    }

    /// Returns the comments who are within the range
    pub fn comments_in_range(&self, range: TextRange) -> &[TextRange] {
        let start = self
            .raw
            .partition_point(|comment| comment.start() < range.start());
        // We expect there are few comments, so switching to find should be faster
        match self.raw[start..]
            .iter()
            .find_position(|comment| comment.end() > range.end())
        {
            Some((in_range, _element)) => &self.raw[start..start + in_range],
            None => &self.raw[start..],
        }
    }
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
