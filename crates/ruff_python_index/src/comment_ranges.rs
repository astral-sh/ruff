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

#[cfg(test)]
mod tests {
    use ruff_python_parser::lexer::LexResult;
    use ruff_python_parser::{tokenize, Mode};
    use ruff_source_file::Locator;
    use ruff_text_size::TextSize;

    use crate::Indexer;

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
