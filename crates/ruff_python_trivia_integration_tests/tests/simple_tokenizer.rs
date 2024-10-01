use insta::assert_debug_snapshot;

use ruff_python_parser::{parse_unchecked, Mode};
use ruff_python_trivia::{lines_after, lines_before, CommentRanges, SimpleToken, SimpleTokenizer};
use ruff_python_trivia::{BackwardsTokenizer, SimpleTokenKind};
use ruff_text_size::{TextLen, TextRange, TextSize};

struct TokenizationTestCase {
    source: &'static str,
    range: TextRange,
    tokens: Vec<SimpleToken>,
}

impl TokenizationTestCase {
    fn assert_reverse_tokenization(&self) {
        let mut backwards = self.tokenize_reverse();

        // Re-reverse to get the tokens in forward order.
        backwards.reverse();

        assert_eq!(&backwards, &self.tokens);
    }

    fn tokenize_reverse(&self) -> Vec<SimpleToken> {
        let parsed = parse_unchecked(self.source, Mode::Module);
        let comment_ranges = CommentRanges::from(parsed.tokens());
        BackwardsTokenizer::new(self.source, self.range, &comment_ranges).collect()
    }

    fn tokens(&self) -> &[SimpleToken] {
        &self.tokens
    }
}

fn tokenize_range(source: &'static str, range: TextRange) -> TokenizationTestCase {
    let tokens: Vec<_> = SimpleTokenizer::new(source, range).collect();

    TokenizationTestCase {
        source,
        range,
        tokens,
    }
}

fn tokenize(source: &'static str) -> TokenizationTestCase {
    tokenize_range(source, TextRange::new(TextSize::new(0), source.text_len()))
}

#[test]
fn tokenize_trivia() {
    let source = "# comment\n    # comment";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_parentheses() {
    let source = "([{}])";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_comma() {
    let source = ",,,,";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_eq() {
    // Should tokenize as `==`, then `=`, regardless of whether we're lexing forwards or
    // backwards.
    let source = "===";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_not_eq() {
    // Should tokenize as `!=`, then `=`, regardless of whether we're lexing forwards or
    // backwards.
    let source = "!==";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_continuation() {
    let source = "( \\\n )";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_operators() {
    let source = "-> *= ( -= ) ~ // ** **= ^ ^= | |=";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_invalid_operators() {
    let source = "-> $=";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());

    // note: not reversible: [other, bogus, bogus] vs [bogus, bogus, other]
}

#[test]
fn tricky_unicode() {
    let source = "មុ";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn identifier_ending_in_non_start_char() {
    let source = "i5";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn string_with_kind() {
    let source = "f'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());

    // note: not reversible: [other, bogus] vs [bogus, other]
}

#[test]
fn string_with_byte_kind() {
    let source = "BR'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());

    // note: not reversible: [other, bogus] vs [bogus, other]
}

#[test]
fn string_with_invalid_kind() {
    let source = "abc'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());

    // note: not reversible: [other, bogus] vs [bogus, other]
}

#[test]
fn identifier_starting_with_string_kind() {
    let source = "foo bar";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn ignore_word_with_only_id_continuing_chars() {
    let source = "555";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());

    // note: not reversible: [other, bogus, bogus] vs [bogus, bogus, other]
}

#[test]
fn tokenize_multichar() {
    let source = "if in else match";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_substring() {
    let source = "('some string') # comment";

    let test_case = tokenize_range(source, TextRange::new(TextSize::new(14), source.text_len()));

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_slash() {
    let source = r" # trailing positional comment
        # Positional arguments only after here
        ,/";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    test_case.assert_reverse_tokenization();
}

#[test]
fn tokenize_bogus() {
    let source = r#"# leading comment
        "a string"
        a = (10)"#;

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
    assert_debug_snapshot!("Reverse", test_case.tokenize_reverse());
}

#[test]
fn single_quoted_multiline_string_containing_comment() {
    let test_case = tokenize(
        r"'This string contains a hash looking like a comment\
# This is not a comment'",
    );

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn single_quoted_multiline_string_implicit_concatenation() {
    let test_case = tokenize(
        r#"'This string contains a hash looking like a comment\
# This is' "not_a_comment""#,
    );

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn triple_quoted_multiline_string_containing_comment() {
    let test_case = tokenize(
        r"'''This string contains a hash looking like a comment
# This is not a comment'''",
    );

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn comment_containing_triple_quoted_string() {
    let test_case = tokenize("'''leading string''' # a comment '''not a string'''");

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn comment_containing_single_quoted_string() {
    let test_case = tokenize("'leading string' # a comment 'not a string'");

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn string_followed_by_multiple_comments() {
    let test_case =
        tokenize(r#"'a string # containing a hash " # and another hash ' # finally a comment"#);

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn string_with_escaped_quote() {
    let test_case = tokenize(r"'a string \' # containing a hash ' # finally a comment");

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn string_with_double_escaped_backslash() {
    let test_case = tokenize(r"'a string \\' # a comment '");

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn empty_string_literal() {
    let test_case = tokenize(r"'' # a comment '");

    assert_debug_snapshot!(test_case.tokenize_reverse());
}

#[test]
fn lines_before_empty_string() {
    assert_eq!(lines_before(TextSize::new(0), ""), 0);
}

#[test]
fn lines_before_in_the_middle_of_a_line() {
    assert_eq!(lines_before(TextSize::new(4), "a = 20"), 0);
}

#[test]
fn lines_before_on_a_new_line() {
    assert_eq!(lines_before(TextSize::new(7), "a = 20\nb = 10"), 1);
}

#[test]
fn lines_before_multiple_leading_newlines() {
    assert_eq!(lines_before(TextSize::new(9), "a = 20\n\r\nb = 10"), 2);
}

#[test]
fn lines_before_with_comment_offset() {
    assert_eq!(lines_before(TextSize::new(8), "a = 20\n# a comment"), 0);
}

#[test]
fn lines_before_with_trailing_comment() {
    assert_eq!(
        lines_before(TextSize::new(22), "a = 20 # some comment\nb = 10"),
        1
    );
}

#[test]
fn lines_before_with_comment_only_line() {
    assert_eq!(
        lines_before(TextSize::new(22), "a = 20\n# some comment\nb = 10"),
        1
    );
}

#[test]
fn lines_after_empty_string() {
    assert_eq!(lines_after(TextSize::new(0), ""), 0);
}

#[test]
fn lines_after_in_the_middle_of_a_line() {
    assert_eq!(lines_after(TextSize::new(4), "a = 20"), 0);
}

#[test]
fn lines_after_before_a_new_line() {
    assert_eq!(lines_after(TextSize::new(6), "a = 20\nb = 10"), 1);
}

#[test]
fn lines_after_multiple_newlines() {
    assert_eq!(lines_after(TextSize::new(6), "a = 20\n\r\nb = 10"), 2);
}

#[test]
fn lines_after_before_comment_offset() {
    assert_eq!(lines_after(TextSize::new(7), "a = 20 # a comment\n"), 0);
}

#[test]
fn lines_after_with_comment_only_line() {
    assert_eq!(
        lines_after(TextSize::new(6), "a = 20\n# some comment\nb = 10"),
        1
    );
}

#[test]
fn test_previous_token_simple() {
    let cases = &["x = (", "x = ( ", "x = (\n"];
    for source in cases {
        let token = BackwardsTokenizer::up_to(source.text_len(), source, &[])
            .skip_trivia()
            .next()
            .unwrap();
        assert_eq!(
            token,
            SimpleToken {
                kind: SimpleTokenKind::LParen,
                range: TextRange::new(TextSize::new(4), TextSize::new(5)),
            }
        );
    }
}
