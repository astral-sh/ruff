use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use itertools::Itertools;
use ruff_source_file::Locator;

use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::{has_leading_content, has_trailing_content, is_python_whitespace};

/// Stores the ranges of comments sorted by [`TextRange::start`] in increasing order. No two ranges are overlapping.
#[derive(Clone, Default)]
pub struct CommentRanges {
    raw: Vec<TextRange>,
}

impl CommentRanges {
    pub fn new(ranges: Vec<TextRange>) -> Self {
        Self { raw: ranges }
    }

    /// Returns `true` if the given range intersects with any comment range.
    pub fn intersects(&self, target: TextRange) -> bool {
        self.raw
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

    /// Returns `true` if a statement or expression includes at least one comment.
    pub fn has_comments<T>(&self, node: &T, locator: &Locator) -> bool
    where
        T: Ranged,
    {
        let start = if has_leading_content(node.start(), locator) {
            node.start()
        } else {
            locator.line_start(node.start())
        };
        let end = if has_trailing_content(node.end(), locator) {
            node.end()
        } else {
            locator.line_end(node.end())
        };

        self.intersects(TextRange::new(start, end))
    }

    /// Given a [`CommentRanges`], determine which comments are grouped together
    /// in "comment blocks". A "comment block" is a sequence of consecutive
    /// own-line comments in which the comment hash (`#`) appears in the same
    /// column in each line, and at least one comment is non-empty.
    ///
    /// Returns a sorted vector containing the offset of the leading hash (`#`)
    /// for each comment in any block comment.
    ///
    /// ## Examples
    /// ```python
    /// # This is a block comment
    /// # because it spans multiple lines
    ///
    ///     # This is also a block comment
    ///     # even though it is indented
    ///
    /// # this is not a block comment
    ///
    /// x = 1  # this is not a block comment because
    /// y = 2  # the lines do not *only* contain comments
    ///
    /// # This is not a block comment because
    ///     # not all consecutive lines have the
    /// # first `#` character in the same column
    ///
    /// """
    /// # This is not a block comment because it is
    /// # contained within a multi-line string/comment
    /// """
    /// ```
    pub fn block_comments(&self, locator: &Locator) -> Vec<TextSize> {
        let mut block_comments: Vec<TextSize> = Vec::new();

        let mut current_block: Vec<TextSize> = Vec::new();
        let mut current_block_column: Option<TextSize> = None;
        let mut current_block_non_empty = false;

        let mut prev_line_end = None;

        for comment_range in &self.raw {
            let offset = comment_range.start();
            let line_start = locator.line_start(offset);
            let line_end = locator.full_line_end(offset);
            let column = offset - line_start;

            // If this is an end-of-line comment, reset the current block.
            if !Self::is_own_line(offset, locator) {
                // Push the current block, and reset.
                if current_block.len() > 1 && current_block_non_empty {
                    block_comments.extend(current_block);
                }
                current_block = vec![];
                current_block_column = None;
                current_block_non_empty = false;
                prev_line_end = Some(line_end);
                continue;
            }

            // If there's a blank line between this comment and the previous
            // comment, reset the current block.
            if prev_line_end.is_some_and(|prev_line_end| {
                locator.contains_line_break(TextRange::new(prev_line_end, line_start))
            }) {
                // Push the current block.
                if current_block.len() > 1 && current_block_non_empty {
                    block_comments.extend(current_block);
                }

                // Reset the block state.
                current_block = vec![offset];
                current_block_column = Some(column);
                current_block_non_empty = !Self::is_empty(*comment_range, locator);
                prev_line_end = Some(line_end);
                continue;
            }

            if let Some(current_column) = current_block_column {
                if column == current_column {
                    // Add the comment to the current block.
                    current_block.push(offset);
                    current_block_non_empty |= !Self::is_empty(*comment_range, locator);
                    prev_line_end = Some(line_end);
                } else {
                    // Push the current block.
                    if current_block.len() > 1 && current_block_non_empty {
                        block_comments.extend(current_block);
                    }

                    // Reset the block state.
                    current_block = vec![offset];
                    current_block_column = Some(column);
                    current_block_non_empty = !Self::is_empty(*comment_range, locator);
                    prev_line_end = Some(line_end);
                }
            } else {
                // Push the current block.
                if current_block.len() > 1 && current_block_non_empty {
                    block_comments.extend(current_block);
                }

                // Reset the block state.
                current_block = vec![offset];
                current_block_column = Some(column);
                current_block_non_empty = !Self::is_empty(*comment_range, locator);
                prev_line_end = Some(line_end);
            }
        }

        // Push any lingering blocks.
        if current_block.len() > 1 && current_block_non_empty {
            block_comments.extend(current_block);
        }

        block_comments
    }

    /// Returns `true` if the given range is an empty comment.
    fn is_empty(range: TextRange, locator: &Locator) -> bool {
        locator
            .slice(range)
            .chars()
            .skip(1)
            .all(is_python_whitespace)
    }

    /// Returns `true` if a comment is an own-line comment (as opposed to an end-of-line comment).
    fn is_own_line(offset: TextSize, locator: &Locator) -> bool {
        let range = TextRange::new(locator.line_start(offset), offset);
        locator.slice(range).chars().all(is_python_whitespace)
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
    type Item = TextRange;
    type IntoIter = std::iter::Copied<std::slice::Iter<'a, TextRange>>;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter().copied()
    }
}
