use ruff_python_parser::{Token, TokenKind};
use ruff_text_size::{Ranged, TextRange};

/// Stores the range of all multiline strings in a file sorted by
/// [`TextRange::start`].
pub struct MultilineRanges {
    ranges: Vec<TextRange>,
}

impl MultilineRanges {
    /// Returns `true` if the given range is inside a multiline string.
    pub fn contains_range(&self, target: TextRange) -> bool {
        self.ranges
            .binary_search_by(|range| {
                if range.contains_range(target) {
                    std::cmp::Ordering::Equal
                } else if range.end() < target.start() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .is_ok()
    }

    /// Returns `true` if the given range intersects with any multiline string.
    pub fn intersects(&self, target: TextRange) -> bool {
        self.ranges
            .binary_search_by(|range| {
                if target.intersect(*range).is_some() {
                    std::cmp::Ordering::Equal
                } else if range.end() < target.start() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .is_ok()
    }
}

#[derive(Default)]
pub(crate) struct MultilineRangesBuilder {
    ranges: Vec<TextRange>,
}

impl MultilineRangesBuilder {
    pub(crate) fn visit_token(&mut self, token: &Token) {
        if matches!(token.kind(), TokenKind::String | TokenKind::FStringMiddle) {
            if token.is_triple_quoted_string() {
                self.ranges.push(token.range());
            }
        }
    }

    pub(crate) fn finish(self) -> MultilineRanges {
        MultilineRanges {
            ranges: self.ranges,
        }
    }
}
