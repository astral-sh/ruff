use insta::assert_debug_snapshot;

use ruff_python_trivia::{
    SimpleToken, SimpleTokenKind, SimpleTokenizer, lines_after, lines_before,
};
use ruff_text_size::{TextLen, TextRange, TextSize};

struct TokenizationTestCase {
    tokens: Vec<SimpleToken>,
}

impl TokenizationTestCase {
    fn tokens(&self) -> &[SimpleToken] {
        &self.tokens
    }
}

fn tokenize_range(source: &'static str, range: TextRange) -> TokenizationTestCase {
    let tokens: Vec<_> = SimpleTokenizer::new(source, range).collect();

    TokenizationTestCase { tokens }
}

fn tokenize(source: &'static str) -> TokenizationTestCase {
    tokenize_range(source, TextRange::new(TextSize::new(0), source.text_len()))
}

#[test]
fn tokenize_trivia() {
    let source = "# comment\n    # comment";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_parentheses() {
    let source = "([{}])";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_comma() {
    let source = ",,,,";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_eq() {
    // Should tokenize as `==`, then `=`.
    let source = "===";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_not_eq() {
    // Should tokenize as `!=`, then `=`.
    let source = "!==";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_continuation() {
    let source = "( \\\n )";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_operators() {
    let source = "-> *= ( -= ) ~ // ** **= ^ ^= | |=";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_invalid_operators() {
    let source = "-> $=";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tricky_unicode() {
    let source = "មុ";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn identifier_ending_in_non_start_char() {
    let source = "i5";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn string_with_kind() {
    let source = "f'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn string_with_byte_kind() {
    let source = "BR'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn fstring() {
    let source = "f'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tstring() {
    let source = "t'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn string_with_invalid_kind() {
    let source = "abc'foo'";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn identifier_starting_with_string_kind() {
    let source = "foo bar";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn ignore_word_with_only_id_continuing_chars() {
    let source = "555";

    let test_case = tokenize(source);
    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_multichar() {
    let source = "if in else match";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_substring() {
    let source = "('some string') # comment";

    let test_case = tokenize_range(source, TextRange::new(TextSize::new(14), source.text_len()));

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_slash() {
    let source = r" # trailing positional comment
        # Positional arguments only after here
        ,/";

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn tokenize_bogus() {
    let source = r#"# leading comment
        "a string"
        a = (10)"#;

    let test_case = tokenize(source);

    assert_debug_snapshot!(test_case.tokens());
}

#[test]
fn bogus_token_stays_within_requested_range() {
    let source = "a!<<bbbb";
    let range = TextRange::new(1.into(), 4.into());
    let tokens: Vec<_> = SimpleTokenizer::new(source, range)
        .map(|token| (token.kind, token.range))
        .collect();

    assert_eq!(
        tokens,
        [
            (SimpleTokenKind::Other, TextRange::new(1.into(), 2.into())),
            (SimpleTokenKind::Bogus, TextRange::new(2.into(), 4.into())),
        ]
    );
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
