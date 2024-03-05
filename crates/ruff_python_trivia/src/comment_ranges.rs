use std::fmt::{Debug, Formatter};
use std::ops::Deref;

use itertools::Itertools;
use ruff_source_file::Locator;

use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::is_python_whitespace;

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
    type Item = &'a TextRange;
    type IntoIter = std::slice::Iter<'a, TextRange>;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter()
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_index::Indexer;
    use ruff_python_parser::lexer::LexResult;
    use ruff_python_parser::{tokenize, Mode};
    use ruff_source_file::Locator;
    use ruff_text_size::TextSize;

    #[test]
    fn block_comments_two_line_block_at_start() {
        // arrange
        let source = "# line 1\n# line 2\n";
        let tokens = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, vec![TextSize::new(0), TextSize::new(9)]);
    }

    #[test]
    fn block_comments_indented_block() {
        // arrange
        let source = "    # line 1\n    # line 2\n";
        let tokens = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, vec![TextSize::new(4), TextSize::new(17)]);
    }

    #[test]
    fn block_comments_single_line_is_not_a_block() {
        // arrange
        let source = "\n";
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<TextSize>::new());
    }

    #[test]
    fn block_comments_lines_with_code_not_a_block() {
        // arrange
        let source = "x = 1  # line 1\ny = 2  # line 2\n";
        let tokens = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<TextSize>::new());
    }

    #[test]
    fn block_comments_sequential_lines_not_in_block() {
        // arrange
        let source = "    # line 1\n        # line 2\n";
        let tokens = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<TextSize>::new());
    }

    #[test]
    fn block_comments_lines_in_triple_quotes_not_a_block() {
        // arrange
        let source = r#"
        """
        # line 1
        # line 2
        """
        "#;
        let tokens = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<TextSize>::new());
    }

    #[test]
    fn block_comments_stress_test() {
        // arrange
        let source = r#"
# block comment 1 line 1
# block comment 2 line 2

# these lines
    # do not form
# a block comment

x = 1  # these lines also do not
y = 2  # do not form a block comment

# these lines do form a block comment
#

    #
    # and so do these
    #

"""
# these lines are in triple quotes and
# therefore do not form a block comment
"""
        "#;
        let tokens = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(
            block_comments,
            vec![
                // Block #1
                TextSize::new(1),
                TextSize::new(26),
                // Block #2
                TextSize::new(174),
                TextSize::new(212),
                // Block #3
                TextSize::new(219),
                TextSize::new(225),
                TextSize::new(247)
            ]
        );
    }
}
