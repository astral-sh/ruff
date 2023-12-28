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

    /// Given a `CommentRanges`, determine which comments are grouped together
    /// in block comments. Block comments are defined as a sequence of consecutive
    /// lines which only contain comments and which all contain the first
    /// `#` character in the same column.
    ///
    /// Returns a vector of vectors, with each representing a block comment, and
    /// the values of each being the offset of the leading `#` character of the comment.
    ///
    /// Example:
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
    pub fn block_comments(&self, locator: &Locator) -> Vec<Vec<TextSize>> {
        let mut block_comments: Vec<Vec<TextSize>> = Vec::new();

        let mut current_block: Vec<TextSize> = Vec::new();
        let mut current_block_column: Option<u32> = None;
        let mut current_block_offset: Option<u32> = None;

        for comment_range in &self.raw {
            let offset = comment_range.start();
            let line_start = locator.line_start(offset);

            if Self::is_end_of_line(locator, line_start) {
                if current_block.len() > 1 {
                    block_comments.push(current_block);
                    current_block = vec![];
                    current_block_column = None;
                    current_block_offset = None;
                }
                continue;
            }

            let line_end = locator.full_line_end(offset).to_u32();
            let column = (offset - line_start).to_u32();

            if let Some(c) = current_block_column {
                if let Some(o) = current_block_offset {
                    if column == c && line_start.to_u32() == o {
                        current_block.push(offset);
                        current_block_offset = Some(line_end);
                    } else {
                        if current_block.len() > 1 {
                            block_comments.push(current_block);
                        }
                        current_block = vec![offset];
                        current_block_column = Some(column);
                        current_block_offset = Some(line_end);
                    }
                }
            } else {
                current_block = vec![offset];
                current_block_column = Some(column);
                current_block_offset = Some(line_end);
            }
        }
        if current_block.len() > 1 {
            block_comments.push(current_block);
        }
        block_comments
    }

    /// Returns `true` if a comment is an end-of-line comment (as opposed to an own-line comment).
    fn is_end_of_line(locator: &Locator, offset_line_start: TextSize) -> bool {
        let contents = locator.full_line(offset_line_start);
        for char in contents.chars() {
            if char == '#' || char == '\r' || char == '\n' {
                return false;
            } else if !is_python_whitespace(char) {
                return true;
            }
        }
        false
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
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(
            block_comments,
            vec![vec![TextSize::new(0), TextSize::new(9)]]
        );
    }

    #[test]
    fn block_comments_indented_block() {
        // arrange
        let source = "    # line 1\n    # line 2\n";
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(
            block_comments,
            vec![vec![TextSize::new(4), TextSize::new(17)]]
        );
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
        assert_eq!(block_comments, Vec::<Vec<TextSize>>::new());
    }

    #[test]
    fn block_comments_lines_with_code_not_a_block() {
        // arrange
        let source = "x = 1  # line 1\ny = 2  # line 2\n";
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<Vec<TextSize>>::new());
    }

    #[test]
    fn block_comments_sequential_lines_not_in_block() {
        // arrange
        let source = "    # line 1\n        # line 2\n";
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<Vec<TextSize>>::new());
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
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(block_comments, Vec::<Vec<TextSize>>::new());
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
        let tokens: Vec<LexResult> = tokenize(source, Mode::Module);
        let locator = Locator::new(source);
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // act
        let block_comments = indexer.comment_ranges().block_comments(&locator);

        // assert
        assert_eq!(
            block_comments,
            vec![
                vec![TextSize::new(1), TextSize::new(26)],
                vec![TextSize::new(174), TextSize::new(212)],
                vec![TextSize::new(219), TextSize::new(225), TextSize::new(247)]
            ]
        );
    }
}
