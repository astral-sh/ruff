use std::collections::BTreeMap;

use ruff_python_parser::{Token, TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// Stores the ranges of all f-strings in a file sorted by [`TextRange::start`].
/// There can be multiple overlapping ranges for nested f-strings.
///
/// Note that the ranges for all unterminated f-strings are not stored.
#[derive(Debug)]
pub struct FStringRanges {
    // Mapping from the f-string start location to its range.
    raw: BTreeMap<TextSize, TextRange>,
}

impl FStringRanges {
    /// Returns `true` if the given range intersects with any f-string range.
    pub fn intersects(&self, target: TextRange) -> bool {
        self.raw
            .values()
            .take_while(|range| range.start() < target.end())
            .any(|range| target.intersect(*range).is_some())
    }

    /// Return the [`TextRange`] of the innermost f-string at the given offset.
    pub fn innermost(&self, offset: TextSize) -> Option<TextRange> {
        self.raw
            .range(..=offset)
            .rev()
            .find(|(_, range)| range.contains(offset))
            .map(|(_, range)| *range)
    }

    /// Return the [`TextRange`] of the outermost f-string at the given offset.
    pub fn outermost(&self, offset: TextSize) -> Option<TextRange> {
        // Explanation of the algorithm:
        //
        // ```python
        // #                                                     v
        //   f"normal" f"another" f"first {f"second {f"third"} second"} first"
        // #                                         ^^(1)^^^
        // #                               ^^^^^^^^^^^^(2)^^^^^^^^^^^^
        // #                      ^^^^^^^^^^^^^^^^^^^^^(3)^^^^^^^^^^^^^^^^^^^^
        // #           ^^^(4)^^^^
        // # ^^^(5)^^^
        // ```
        //
        // The offset is marked with a `v` and the ranges are numbered in the order
        // they are yielded by the iterator in the reverse order. The algorithm
        // works as follows:
        //   1. Skip all ranges that don't contain the offset (1).
        //   2. Take all ranges that contain the offset (2, 3).
        //   3. Stop taking ranges when the offset is no longer contained.
        //   4. Take the last range that contained the offset (3, the outermost).
        self.raw
            .range(..=offset)
            .rev()
            .skip_while(|(_, range)| !range.contains(offset))
            .take_while(|(_, range)| range.contains(offset))
            .last()
            .map(|(_, range)| *range)
    }

    /// Returns an iterator over all f-string [`TextRange`] sorted by their
    /// start location.
    ///
    /// For nested f-strings, the outermost f-string is yielded first, moving
    /// inwards with each iteration.
    #[inline]
    pub fn values(&self) -> impl Iterator<Item = &TextRange> + '_ {
        self.raw.values()
    }

    /// Returns the number of f-string ranges stored.
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }
}

#[derive(Default)]
pub(crate) struct FStringRangesBuilder {
    start_locations: Vec<TextSize>,
    raw: BTreeMap<TextSize, TextRange>,
}

impl FStringRangesBuilder {
    pub(crate) fn visit_token(&mut self, token: &Token) {
        match token.kind() {
            TokenKind::FStringStart => {
                self.start_locations.push(token.start());
            }
            TokenKind::FStringEnd => {
                if let Some(start) = self.start_locations.pop() {
                    self.raw.insert(start, TextRange::new(start, token.end()));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn finish(self) -> FStringRanges {
        FStringRanges { raw: self.raw }
    }
}
