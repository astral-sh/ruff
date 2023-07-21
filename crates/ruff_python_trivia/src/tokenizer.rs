use memchr::{memchr2, memchr3, memrchr3_iter};
use ruff_text_size::{TextLen, TextRange, TextSize};
use unic_ucd_ident::{is_xid_continue, is_xid_start};

use crate::{is_python_whitespace, Cursor};

/// Searches for the first non-trivia character in `range`.
///
/// The search skips over any whitespace and comments.
///
/// Returns `Some` if the range contains any non-trivia character. The first item is the absolute offset
/// of the character, the second item the non-trivia character.
///
/// Returns `None` if the range is empty or only contains trivia (whitespace or comments).
pub fn first_non_trivia_token(offset: TextSize, code: &str) -> Option<SimpleToken> {
    SimpleTokenizer::starts_at(offset, code)
        .skip_trivia()
        .next()
}

/// Returns the number of newlines between `offset` and the first non whitespace character in the source code.
pub fn lines_before(offset: TextSize, code: &str) -> u32 {
    let mut cursor = Cursor::new(&code[TextRange::up_to(offset)]);

    let mut newlines = 0u32;
    while let Some(c) = cursor.bump_back() {
        match c {
            '\n' => {
                cursor.eat_char_back('\r');
                newlines += 1;
            }
            '\r' => {
                newlines += 1;
            }
            c if is_python_whitespace(c) => {
                continue;
            }
            _ => {
                break;
            }
        }
    }

    newlines
}

/// Counts the empty lines between `offset` and the first non-whitespace character.
pub fn lines_after(offset: TextSize, code: &str) -> u32 {
    let mut cursor = Cursor::new(&code[offset.to_usize()..]);

    let mut newlines = 0u32;
    while let Some(c) = cursor.bump() {
        match c {
            '\n' => {
                newlines += 1;
            }
            '\r' => {
                cursor.eat_char('\n');
                newlines += 1;
            }
            c if is_python_whitespace(c) => {
                continue;
            }
            _ => {
                break;
            }
        }
    }

    newlines
}

/// Returns the position after skipping any trailing trivia up to, but not including the newline character.
pub fn skip_trailing_trivia(offset: TextSize, code: &str) -> TextSize {
    let tokenizer = SimpleTokenizer::starts_at(offset, code);

    for token in tokenizer {
        match token.kind() {
            SimpleTokenKind::Whitespace
            | SimpleTokenKind::Comment
            | SimpleTokenKind::Continuation => {
                // No op
            }
            _ => {
                return token.start();
            }
        }
    }

    offset
}

fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || is_non_ascii_identifier_start(c)
}

// Checks if the character c is a valid continuation character as described
// in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
fn is_identifier_continuation(c: char) -> bool {
    if c.is_ascii() {
        matches!(c, 'a'..='z' | 'A'..='Z' | '_' | '0'..='9')
    } else {
        is_xid_continue(c)
    }
}

fn is_non_ascii_identifier_start(c: char) -> bool {
    is_xid_start(c)
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SimpleToken {
    pub kind: SimpleTokenKind,
    pub range: TextRange,
}

impl SimpleToken {
    pub const fn kind(&self) -> SimpleTokenKind {
        self.kind
    }

    #[allow(unused)]
    pub const fn range(&self) -> TextRange {
        self.range
    }

    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub const fn end(&self) -> TextSize {
        self.range.end()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SimpleTokenKind {
    /// A comment, not including the trailing new line.
    Comment,

    /// Sequence of ' ' or '\t'
    Whitespace,

    /// Start or end of the file
    EndOfFile,

    /// `\\`
    Continuation,

    /// `\n` or `\r` or `\r\n`
    Newline,

    /// `(`
    LParen,

    /// `)`
    RParen,

    /// `{`
    LBrace,

    /// `}`
    RBrace,

    /// `[`
    LBracket,

    /// `]`
    RBracket,

    /// `,`
    Comma,

    /// `:`
    Colon,

    /// '/'
    Slash,

    /// '*'
    Star,

    /// `.`.
    Dot,

    /// `else`
    Else,

    /// `if`
    If,

    /// `in`
    In,

    /// `as`
    As,

    /// `match`
    Match,

    /// `with`
    With,

    /// `async`
    Async,

    /// Any other non trivia token.
    Other,

    /// Returned for each character after [`SimpleTokenKind::Other`] has been returned once.
    Bogus,
}

impl SimpleTokenKind {
    const fn from_non_trivia_char(c: char) -> SimpleTokenKind {
        match c {
            '(' => SimpleTokenKind::LParen,
            ')' => SimpleTokenKind::RParen,
            '[' => SimpleTokenKind::LBracket,
            ']' => SimpleTokenKind::RBracket,
            '{' => SimpleTokenKind::LBrace,
            '}' => SimpleTokenKind::RBrace,
            ',' => SimpleTokenKind::Comma,
            ':' => SimpleTokenKind::Colon,
            '/' => SimpleTokenKind::Slash,
            '*' => SimpleTokenKind::Star,
            '.' => SimpleTokenKind::Dot,
            _ => SimpleTokenKind::Other,
        }
    }

    const fn is_trivia(self) -> bool {
        matches!(
            self,
            SimpleTokenKind::Whitespace
                | SimpleTokenKind::Newline
                | SimpleTokenKind::Comment
                | SimpleTokenKind::Continuation
        )
    }
}

/// Simple zero allocation tokenizer for tokenizing trivia (and some tokens).
///
/// The tokenizer must start at an offset that is trivia (e.g. not inside of a multiline string).
///
/// The tokenizer doesn't guarantee any correctness after it returned a [`SimpleTokenKind::Other`]. That's why it
/// will return [`SimpleTokenKind::Bogus`] for every character after until it reaches the end of the file.
pub struct SimpleTokenizer<'a> {
    offset: TextSize,
    back_offset: TextSize,
    /// `true` when it is known that the current `back` line has no comment for sure.
    back_line_has_no_comment: bool,
    bogus: bool,
    source: &'a str,
    cursor: Cursor<'a>,
}

impl<'a> SimpleTokenizer<'a> {
    pub fn new(source: &'a str, range: TextRange) -> Self {
        Self {
            offset: range.start(),
            back_offset: range.end(),
            back_line_has_no_comment: false,
            bogus: false,
            source,
            cursor: Cursor::new(&source[range]),
        }
    }

    pub fn starts_at(offset: TextSize, source: &'a str) -> Self {
        let range = TextRange::new(offset, source.text_len());
        Self::new(source, range)
    }

    /// Creates a tokenizer that lexes tokens from the start of `source` up to `offset`.
    ///
    /// Consider using [`SimpleTokenizer::up_to_without_back_comment`] if intend to lex backwards.
    pub fn up_to(offset: TextSize, source: &'a str) -> Self {
        Self::new(source, TextRange::up_to(offset))
    }

    /// Creates a tokenizer that lexes tokens from the start of `source` up to `offset`, and informs
    /// the lexer that the line at `offset` contains no comments. This can significantly speed up backwards lexing
    /// because the lexer doesn't need to scan for comments.
    pub fn up_to_without_back_comment(offset: TextSize, source: &'a str) -> Self {
        let mut tokenizer = Self::up_to(offset, source);
        tokenizer.back_line_has_no_comment = true;
        tokenizer
    }

    fn to_keyword_or_other(&self, range: TextRange) -> SimpleTokenKind {
        let source = &self.source[range];
        match source {
            "as" => SimpleTokenKind::As,
            "async" => SimpleTokenKind::Async,
            "else" => SimpleTokenKind::Else,
            "if" => SimpleTokenKind::If,
            "in" => SimpleTokenKind::In,
            "match" => SimpleTokenKind::Match, // Match is a soft keyword that depends on the context but we can always lex it as a keyword and leave it to the caller (parser) to decide if it should be handled as an identifier or keyword.
            "with" => SimpleTokenKind::With,
            // ...,
            _ => SimpleTokenKind::Other, // Potentially an identifier, but only if it isn't a string prefix. We can ignore this for now https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
        }
    }

    fn next_token(&mut self) -> SimpleToken {
        self.cursor.start_token();

        let Some(first) = self.cursor.bump() else {
            return SimpleToken {
                kind: SimpleTokenKind::EndOfFile,
                range: TextRange::empty(self.offset),
            };
        };

        if self.bogus {
            let token = SimpleToken {
                kind: SimpleTokenKind::Bogus,
                range: TextRange::at(self.offset, first.text_len()),
            };

            self.offset += first.text_len();
            return token;
        }

        let kind = match first {
            ' ' | '\t' => {
                self.cursor.eat_while(|c| matches!(c, ' ' | '\t'));
                SimpleTokenKind::Whitespace
            }

            '\n' => SimpleTokenKind::Newline,

            '\r' => {
                self.cursor.eat_char('\n');
                SimpleTokenKind::Newline
            }

            '#' => {
                self.cursor.eat_while(|c| !matches!(c, '\n' | '\r'));
                SimpleTokenKind::Comment
            }

            '\\' => SimpleTokenKind::Continuation,

            c => {
                let kind = if is_identifier_start(c) {
                    self.cursor.eat_while(is_identifier_continuation);
                    let token_len = self.cursor.token_len();

                    let range = TextRange::at(self.offset, token_len);
                    self.to_keyword_or_other(range)
                } else {
                    SimpleTokenKind::from_non_trivia_char(c)
                };

                if kind == SimpleTokenKind::Other {
                    self.bogus = true;
                }
                kind
            }
        };

        let token_len = self.cursor.token_len();

        let token = SimpleToken {
            kind,
            range: TextRange::at(self.offset, token_len),
        };

        self.offset += token_len;

        token
    }

    /// Returns the next token from the back. Prefer iterating forwards. Iterating backwards is significantly more expensive
    /// because it needs to check if the line has any comments when encountering any non-trivia token.
    pub fn next_token_back(&mut self) -> SimpleToken {
        self.cursor.start_token();

        let Some(last) = self.cursor.bump_back() else {
            return SimpleToken {
                kind: SimpleTokenKind::EndOfFile,
                range: TextRange::empty(self.back_offset),
            };
        };

        if self.bogus {
            let token = SimpleToken {
                kind: SimpleTokenKind::Bogus,
                range: TextRange::at(self.back_offset - last.text_len(), last.text_len()),
            };

            self.back_offset -= last.text_len();
            return token;
        }

        let kind = match last {
            // This may not be 100% correct because it will lex-out trailing whitespace from a comment
            // as whitespace rather than being part of the token. This shouldn't matter for what we use the lexer for.
            ' ' | '\t' => {
                self.cursor.eat_back_while(|c| matches!(c, ' ' | '\t'));
                SimpleTokenKind::Whitespace
            }

            '\r' => {
                self.back_line_has_no_comment = false;
                SimpleTokenKind::Newline
            }

            '\n' => {
                self.back_line_has_no_comment = false;
                self.cursor.eat_char_back('\r');
                SimpleTokenKind::Newline
            }

            // Empty comment (could also be a comment nested in another comment, but this shouldn't matter for what we use the lexer for)
            '#' => SimpleTokenKind::Comment,

            // For all other tokens, test if the character isn't part of a comment.
            c => {
                // Skip the test whether there's a preceding comment if it has been performed before.
                let comment_length = if self.back_line_has_no_comment {
                    None
                } else {
                    let bytes = self.cursor.chars().as_str().as_bytes();
                    let mut potential_comment_starts: smallvec::SmallVec<[TextSize; 2]> =
                        smallvec::SmallVec::new();

                    // Find the start of the line, or any potential comments.
                    for index in memrchr3_iter(b'\n', b'\r', b'#', bytes) {
                        if bytes[index] == b'#' {
                            // Potentially a comment, but not guaranteed
                            // SAFETY: Safe, because ruff only supports files up to 4GB
                            potential_comment_starts.push(TextSize::try_from(index).unwrap());
                        } else {
                            break;
                        }
                    }

                    // No comments
                    if potential_comment_starts.is_empty() {
                        None
                    } else {
                        // The line contains at least one `#` token. The `#` can indicate the start of a
                        // comment, meaning the current token is commented out, or it is a regular `#` inside of a string.
                        self.comment_from_hash_positions(&potential_comment_starts)
                    }
                };

                // From here on it is guaranteed that this line has no other comment.
                self.back_line_has_no_comment = true;

                if let Some(comment_length) = comment_length {
                    // It is a comment, bump all tokens
                    for _ in 0..usize::from(comment_length) {
                        self.cursor.bump_back().unwrap();
                    }

                    SimpleTokenKind::Comment
                } else if c == '\\' {
                    SimpleTokenKind::Continuation
                } else {
                    let kind = if is_identifier_continuation(c) {
                        // if we only have identifier continuations but no start (e.g. 555) we
                        // don't want to consume the chars, so in that case, we want to rewind the
                        // cursor to here
                        let savepoint = self.cursor.clone();
                        self.cursor.eat_back_while(is_identifier_continuation);

                        let token_len = self.cursor.token_len();
                        let range = TextRange::at(self.back_offset - token_len, token_len);

                        if self.source[range]
                            .chars()
                            .next()
                            .is_some_and(is_identifier_start)
                        {
                            self.to_keyword_or_other(range)
                        } else {
                            self.cursor = savepoint;
                            SimpleTokenKind::Other
                        }
                    } else {
                        SimpleTokenKind::from_non_trivia_char(c)
                    };

                    if kind == SimpleTokenKind::Other {
                        self.bogus = true;
                    }

                    kind
                }
            }
        };

        let token_len = self.cursor.token_len();

        let start = self.back_offset - token_len;

        let token = SimpleToken {
            kind,
            range: TextRange::at(start, token_len),
        };

        self.back_offset = start;

        token
    }

    pub fn skip_trivia(self) -> impl Iterator<Item = SimpleToken> + DoubleEndedIterator + 'a {
        self.filter(|t| !t.kind().is_trivia())
    }

    /// Given the position of `#` tokens on a line, test if any `#` is the start of a comment and, if so, return the
    /// length of the comment.
    ///
    /// The challenge is that `#` tokens can also appear inside of strings:
    ///
    /// ```python
    /// ' #not a comment'
    /// ```
    ///
    /// This looks innocent but is the `'` really the start of the new string or could it be a closing delimiter
    /// of a previously started string:
    ///
    /// ```python
    /// ' a string\
    /// ` # a comment '
    /// ```
    ///
    /// The only way to reliability tell whether the `#` is a comment when the comment contains a quote char is
    /// to forward lex all strings and comments and test if there's any unclosed string literal. If so, then
    /// the hash cannot be a comment.
    fn comment_from_hash_positions(&self, hash_positions: &[TextSize]) -> Option<TextSize> {
        // Iterate over the `#` positions from the start to the end of the line.
        // This is necessary to correctly support `a # comment # comment`.
        for possible_start in hash_positions.iter().rev() {
            let comment_bytes =
                self.source[TextRange::new(*possible_start, self.back_offset)].as_bytes();

            // Test if the comment contains any quotes. If so, then it's possible that the `#` token isn't
            // the start of a comment, but instead part of a string:
            // ```python
            // a + 'a string # not a comment'
            // a + '''a string
            // # not a comment'''
            // ```
            match memchr2(b'\'', b'"', comment_bytes) {
                // Most comments don't contain quotes, and most strings don't contain comments.
                // For these it's safe to assume that they are comments.
                None => return Some(self.cursor.chars().as_str().text_len() - possible_start),
                // Now it gets complicated... There's no good way to know whether this is a string or not.
                // It is necessary to lex all strings and comments from the start to know if it is one or the other.
                Some(_) => {
                    if find_unterminated_string_kind(
                        &self.cursor.chars().as_str()[TextRange::up_to(*possible_start)],
                    )
                    .is_none()
                    {
                        // There's no unterminated string at the comment's start position. This *must*
                        // be a comment.
                        return Some(self.cursor.chars().as_str().text_len() - possible_start);
                    }

                    // This is a hash inside of a string: `'test # not a comment'` continue with the next potential comment on the line.
                }
            }
        }

        None
    }
}

fn find_unterminated_string_kind(input: &str) -> Option<StringKind> {
    let mut rest = input;

    while let Some(comment_or_string_start) = memchr3(b'#', b'\'', b'\"', rest.as_bytes()) {
        let c = rest.as_bytes()[comment_or_string_start] as char;
        let after = &rest[comment_or_string_start + 1..];

        if c == '#' {
            let comment_end = memchr2(b'\n', b'\r', after.as_bytes()).unwrap_or(after.len());
            rest = &after[comment_end..];
        } else {
            let mut cursor = Cursor::new(after);
            let quote_kind = if c == '\'' {
                QuoteKind::Single
            } else {
                QuoteKind::Double
            };

            let string_kind = if cursor.eat_char(quote_kind.as_char()) {
                // `''` or `""`
                if cursor.eat_char(quote_kind.as_char()) {
                    // `'''` or `"""`
                    StringKind::Triple(quote_kind)
                } else {
                    // empty string literal, nothing more to lex
                    rest = cursor.chars().as_str();
                    continue;
                }
            } else {
                StringKind::Single(quote_kind)
            };

            if !is_string_terminated(string_kind, &mut cursor) {
                return Some(string_kind);
            }

            rest = cursor.chars().as_str();
        }
    }

    None
}

fn is_string_terminated(kind: StringKind, cursor: &mut Cursor) -> bool {
    let quote_char = kind.quote_kind().as_char();

    while let Some(c) = cursor.bump() {
        match c {
            '\n' | '\r' if kind.is_single() => {
                // Reached the end of the line without a closing quote, this is an unterminated string literal.
                return false;
            }
            '\\' => {
                // Skip over escaped quotes that match this strings quotes or double escaped backslashes
                if cursor.eat_char(quote_char) || cursor.eat_char('\\') {
                    continue;
                }
                // Eat over line continuation
                cursor.eat_char('\r');
                cursor.eat_char('\n');
            }
            c if c == quote_char => {
                if kind.is_single() || (cursor.eat_char(quote_char) && cursor.eat_char(quote_char))
                {
                    return true;
                }
            }
            _ => {
                // continue
            }
        }
    }

    // Reached end without a closing quote
    false
}

impl Iterator for SimpleTokenizer<'_> {
    type Item = SimpleToken;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token();

        if token.kind == SimpleTokenKind::EndOfFile {
            None
        } else {
            Some(token)
        }
    }
}

impl DoubleEndedIterator for SimpleTokenizer<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let token = self.next_token_back();

        if token.kind == SimpleTokenKind::EndOfFile {
            None
        } else {
            Some(token)
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum StringKind {
    /// `'...'` or `"..."`
    Single(QuoteKind),
    /// `'''...'''` or `"""..."""`
    Triple(QuoteKind),
}

impl StringKind {
    const fn quote_kind(self) -> QuoteKind {
        match self {
            StringKind::Single(kind) => kind,
            StringKind::Triple(kind) => kind,
        }
    }

    const fn is_single(self) -> bool {
        matches!(self, StringKind::Single(_))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum QuoteKind {
    /// `'``
    Single,

    /// `"`
    Double,
}

impl QuoteKind {
    const fn as_char(self) -> char {
        match self {
            QuoteKind::Single => '\'',
            QuoteKind::Double => '"',
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use ruff_text_size::{TextLen, TextRange, TextSize};

    use crate::tokenizer::{lines_after, lines_before, SimpleToken, SimpleTokenizer};

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
            SimpleTokenizer::new(self.source, self.range)
                .rev()
                .collect()
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
    fn tokenize_continuation() {
        let source = "( \\\n )";

        let test_case = tokenize(source);

        assert_debug_snapshot!(test_case.tokens());
        test_case.assert_reverse_tokenization();
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

        let test_case =
            tokenize_range(source, TextRange::new(TextSize::new(14), source.text_len()));

        assert_debug_snapshot!(test_case.tokens());
        test_case.assert_reverse_tokenization();
    }

    #[test]
    fn tokenize_slash() {
        let source = r#" # trailing positional comment
        # Positional arguments only after here
        ,/"#;

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
            r#"'This string contains a hash looking like a comment\
# This is not a comment'"#,
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
            r#"'''This string contains a hash looking like a comment
# This is not a comment'''"#,
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
        let test_case = tokenize(r#"'a string \' # containing a hash ' # finally a comment"#);

        assert_debug_snapshot!(test_case.tokenize_reverse());
    }

    #[test]
    fn string_with_double_escaped_backslash() {
        let test_case = tokenize(r#"'a string \\' # a comment '"#);

        assert_debug_snapshot!(test_case.tokenize_reverse());
    }

    #[test]
    fn empty_string_literal() {
        let test_case = tokenize(r#"'' # a comment '"#);

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
}
