use ruff_python_parser::{parse_unchecked, Mode};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::TextSize;

#[test]
fn block_comments_two_line_block_at_start() {
    // arrange
    let source = "# line 1\n# line 2\n";
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

    // assert
    assert_eq!(block_comments, vec![TextSize::new(0), TextSize::new(9)]);
}

#[test]
fn block_comments_indented_block() {
    // arrange
    let source = "    # line 1\n    # line 2\n";
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

    // assert
    assert_eq!(block_comments, vec![TextSize::new(4), TextSize::new(17)]);
}

#[test]
fn block_comments_single_line_is_not_a_block() {
    // arrange
    let source = "\n";
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

    // assert
    assert_eq!(block_comments, Vec::<TextSize>::new());
}

#[test]
fn block_comments_lines_with_code_not_a_block() {
    // arrange
    let source = "x = 1  # line 1\ny = 2  # line 2\n";
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

    // assert
    assert_eq!(block_comments, Vec::<TextSize>::new());
}

#[test]
fn block_comments_sequential_lines_not_in_block() {
    // arrange
    let source = "    # line 1\n        # line 2\n";
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

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
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

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
    let parsed = parse_unchecked(source, Mode::Module);
    let locator = Locator::new(source);
    let comment_ranges = CommentRanges::from(parsed.tokens());

    // act
    let block_comments = comment_ranges.block_comments(&locator);

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
