use ruff_python_parser::Tok;
use ruff_text_size::TextRange;

/// Stores the range of all multiline strings in a file sorted by
/// [`TextRange::start`].
pub struct MultilineRanges {
    ranges: Vec<TextRange>,
}

impl MultilineRanges {
    /// Returns `true` if the given range is inside a multiline string.
    pub fn contains(&self, target: TextRange) -> bool {
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

    /// Returns `true` if the given range contains a multiline string.
    pub fn intersects(&self, target: TextRange) -> bool {
        self.ranges
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
}

#[derive(Default)]
pub(crate) struct MultilineRangesBuilder {
    ranges: Vec<TextRange>,
}

impl MultilineRangesBuilder {
    pub(crate) fn visit_token(&mut self, token: &Tok, range: TextRange) {
        if let Tok::String { triple_quoted, .. } = token {
            if *triple_quoted {
                self.ranges.push(range);
            }
        }
    }

    pub(crate) fn finish(self) -> MultilineRanges {
        MultilineRanges {
            ranges: self.ranges,
        }
    }
}
