use unicode_ident::{is_xid_continue, is_xid_start};

use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

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

/// Returns the only non-trivia, non-closing parenthesis token in `range`.
///
/// Includes debug assertions that the range only contains that single token.
pub fn find_only_token_in_range(
    range: TextRange,
    token_kind: SimpleTokenKind,
    code: &str,
) -> SimpleToken {
    let mut tokens = SimpleTokenizer::new(code, range)
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let token = tokens.next().expect("Expected a token");
    debug_assert_eq!(token.kind(), token_kind);
    let mut tokens = tokens.skip_while(|token| token.kind == SimpleTokenKind::LParen);
    debug_assert_eq!(tokens.next(), None);
    token
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

/// Counts the empty lines after `offset`, ignoring any trailing trivia: end-of-line comments,
/// own-line comments, and any intermediary newlines.
pub fn lines_after_ignoring_trivia(offset: TextSize, code: &str) -> u32 {
    let mut newlines = 0u32;
    for token in SimpleTokenizer::starts_at(offset, code) {
        match token.kind() {
            SimpleTokenKind::Newline => {
                newlines += 1;
            }
            SimpleTokenKind::Whitespace => {}
            // If we see a comment, reset the newlines counter.
            SimpleTokenKind::Comment => {
                newlines = 0;
            }
            // As soon as we see a non-trivia token, we're done.
            _ => {
                break;
            }
        }
    }
    newlines
}

/// Counts the empty lines after `offset`, ignoring any trailing trivia on the same line as
/// `offset`.
#[allow(clippy::cast_possible_truncation)]
pub fn lines_after_ignoring_end_of_line_trivia(offset: TextSize, code: &str) -> u32 {
    // SAFETY: We don't support files greater than 4GB, so casting to u32 is safe.
    SimpleTokenizer::starts_at(offset, code)
        .skip_while(|token| token.kind != SimpleTokenKind::Newline && token.kind.is_trivia())
        .take_while(|token| {
            token.kind == SimpleTokenKind::Newline || token.kind == SimpleTokenKind::Whitespace
        })
        .filter(|token| token.kind == SimpleTokenKind::Newline)
        .count() as u32
}

fn is_identifier_start(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphabetic() || c == '_'
    } else {
        is_xid_start(c)
    }
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

fn to_keyword_or_other(source: &str) -> SimpleTokenKind {
    match source {
        "and" => SimpleTokenKind::And,
        "as" => SimpleTokenKind::As,
        "assert" => SimpleTokenKind::Assert,
        "async" => SimpleTokenKind::Async,
        "await" => SimpleTokenKind::Await,
        "break" => SimpleTokenKind::Break,
        "class" => SimpleTokenKind::Class,
        "continue" => SimpleTokenKind::Continue,
        "def" => SimpleTokenKind::Def,
        "del" => SimpleTokenKind::Del,
        "elif" => SimpleTokenKind::Elif,
        "else" => SimpleTokenKind::Else,
        "except" => SimpleTokenKind::Except,
        "finally" => SimpleTokenKind::Finally,
        "for" => SimpleTokenKind::For,
        "from" => SimpleTokenKind::From,
        "global" => SimpleTokenKind::Global,
        "if" => SimpleTokenKind::If,
        "import" => SimpleTokenKind::Import,
        "in" => SimpleTokenKind::In,
        "is" => SimpleTokenKind::Is,
        "lambda" => SimpleTokenKind::Lambda,
        "nonlocal" => SimpleTokenKind::Nonlocal,
        "not" => SimpleTokenKind::Not,
        "or" => SimpleTokenKind::Or,
        "pass" => SimpleTokenKind::Pass,
        "raise" => SimpleTokenKind::Raise,
        "return" => SimpleTokenKind::Return,
        "try" => SimpleTokenKind::Try,
        "while" => SimpleTokenKind::While,
        "match" => SimpleTokenKind::Match, // Match is a soft keyword that depends on the context but we can always lex it as a keyword and leave it to the caller (parser) to decide if it should be handled as an identifier or keyword.
        "type" => SimpleTokenKind::Type, // Type is a soft keyword that depends on the context but we can always lex it as a keyword and leave it to the caller (parser) to decide if it should be handled as an identifier or keyword.
        "case" => SimpleTokenKind::Case,
        "with" => SimpleTokenKind::With,
        "yield" => SimpleTokenKind::Yield,
        _ => SimpleTokenKind::Other, // Potentially an identifier, but only if it isn't a string prefix. We can ignore this for now https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals
    }
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
}

impl Ranged for SimpleToken {
    fn range(&self) -> TextRange {
        self.range
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

    /// `;`
    Semi,

    /// '/'
    Slash,

    /// '*'
    Star,

    /// `.`.
    Dot,

    /// `+`
    Plus,

    /// `-`
    Minus,

    /// `=`
    Equals,

    /// `>`
    Greater,

    /// `<`
    Less,

    /// `%`
    Percent,

    /// `&`
    Ampersand,

    /// `^`
    Circumflex,

    /// `|`
    Vbar,

    /// `@`
    At,

    /// `~`
    Tilde,

    /// `==`
    EqEqual,

    /// `!=`
    NotEqual,

    /// `<=`
    LessEqual,

    /// `>=`
    GreaterEqual,

    /// `<<`
    LeftShift,

    /// `>>`
    RightShift,

    /// `**`
    DoubleStar,

    /// `**=`
    DoubleStarEqual,

    /// `+=`
    PlusEqual,

    /// `-=`
    MinusEqual,

    /// `*=`
    StarEqual,

    /// `/=`
    SlashEqual,

    /// `%=`
    PercentEqual,

    /// `&=`
    AmperEqual,

    /// `|=`
    VbarEqual,

    /// `^=`
    CircumflexEqual,

    /// `<<=`
    LeftShiftEqual,

    /// `>>=`
    RightShiftEqual,

    /// `//`
    DoubleSlash,

    /// `//=`
    DoubleSlashEqual,

    /// `:=`
    ColonEqual,

    /// `...`
    Ellipsis,

    /// `@=`
    AtEqual,

    /// `->`
    RArrow,

    /// `and`
    And,

    /// `as`
    As,

    /// `assert`
    Assert,

    /// `async`
    Async,

    /// `await`
    Await,

    /// `break`
    Break,

    /// `class`
    Class,

    /// `continue`
    Continue,

    /// `def`
    Def,

    /// `del`
    Del,

    /// `elif`
    Elif,

    /// `else`
    Else,

    /// `except`
    Except,

    /// `finally`
    Finally,

    /// `for`
    For,

    /// `from`
    From,

    /// `global`
    Global,

    /// `if`
    If,

    /// `import`
    Import,

    /// `in`
    In,

    /// `is`
    Is,

    /// `lambda`
    Lambda,

    /// `nonlocal`
    Nonlocal,

    /// `not`
    Not,

    /// `or`
    Or,

    /// `pass`
    Pass,

    /// `raise`
    Raise,

    /// `return`
    Return,

    /// `try`
    Try,

    /// `while`
    While,

    /// `match`
    Match,

    /// `type`
    Type,

    /// `case`
    Case,

    /// `with`
    With,

    /// `yield`
    Yield,

    /// Any other non trivia token.
    Other,

    /// Returned for each character after [`SimpleTokenKind::Other`] has been returned once.
    Bogus,
}

impl SimpleTokenKind {
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

/// Simple zero allocation tokenizer handling most tokens.
///
/// The tokenizer must start at an offset that is trivia (e.g. not inside of a multiline string).
///
/// In case it finds something it can't parse, the tokenizer will return a
/// [`SimpleTokenKind::Other`] and then only a final [`SimpleTokenKind::Bogus`] afterwards.
pub struct SimpleTokenizer<'a> {
    offset: TextSize,
    /// `true` when it is known that the current `back` line has no comment for sure.
    bogus: bool,
    source: &'a str,
    cursor: Cursor<'a>,
}

impl<'a> SimpleTokenizer<'a> {
    pub fn new(source: &'a str, range: TextRange) -> Self {
        Self {
            offset: range.start(),
            bogus: false,
            source,
            cursor: Cursor::new(&source[range]),
        }
    }

    pub fn starts_at(offset: TextSize, source: &'a str) -> Self {
        let range = TextRange::new(offset, source.text_len());
        Self::new(source, range)
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
            // Emit a single final bogus token
            let token = SimpleToken {
                kind: SimpleTokenKind::Bogus,
                range: TextRange::new(self.offset, self.source.text_len()),
            };

            // Set the cursor to EOF
            self.cursor = Cursor::new("");
            self.offset = self.source.text_len();
            return token;
        }

        let kind = self.next_token_inner(first);

        let token_len = self.cursor.token_len();

        let token = SimpleToken {
            kind,
            range: TextRange::at(self.offset, token_len),
        };

        self.offset += token_len;

        token
    }

    fn next_token_inner(&mut self, first: char) -> SimpleTokenKind {
        match first {
            // Keywords and identifiers
            c if is_identifier_start(c) => {
                self.cursor.eat_while(is_identifier_continuation);
                let token_len = self.cursor.token_len();

                let range = TextRange::at(self.offset, token_len);
                let kind = to_keyword_or_other(&self.source[range]);

                if kind == SimpleTokenKind::Other {
                    self.bogus = true;
                }
                kind
            }

            // Space, tab, or form feed. We ignore the true semantics of form feed, and treat it as
            // whitespace.
            ' ' | '\t' | '\x0C' => {
                self.cursor.eat_while(|c| matches!(c, ' ' | '\t' | '\x0C'));
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

            // Non-trivia, non-keyword tokens
            '=' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::EqEqual
                } else {
                    SimpleTokenKind::Equals
                }
            }
            '+' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::PlusEqual
                } else {
                    SimpleTokenKind::Plus
                }
            }
            '*' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::StarEqual
                } else if self.cursor.eat_char('*') {
                    if self.cursor.eat_char('=') {
                        SimpleTokenKind::DoubleStarEqual
                    } else {
                        SimpleTokenKind::DoubleStar
                    }
                } else {
                    SimpleTokenKind::Star
                }
            }
            '/' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::SlashEqual
                } else if self.cursor.eat_char('/') {
                    if self.cursor.eat_char('=') {
                        SimpleTokenKind::DoubleSlashEqual
                    } else {
                        SimpleTokenKind::DoubleSlash
                    }
                } else {
                    SimpleTokenKind::Slash
                }
            }
            '%' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::PercentEqual
                } else {
                    SimpleTokenKind::Percent
                }
            }
            '|' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::VbarEqual
                } else {
                    SimpleTokenKind::Vbar
                }
            }
            '^' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::CircumflexEqual
                } else {
                    SimpleTokenKind::Circumflex
                }
            }
            '&' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::AmperEqual
                } else {
                    SimpleTokenKind::Ampersand
                }
            }
            '-' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::MinusEqual
                } else if self.cursor.eat_char('>') {
                    SimpleTokenKind::RArrow
                } else {
                    SimpleTokenKind::Minus
                }
            }
            '@' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::AtEqual
                } else {
                    SimpleTokenKind::At
                }
            }
            '!' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::NotEqual
                } else {
                    self.bogus = true;
                    SimpleTokenKind::Other
                }
            }
            '~' => SimpleTokenKind::Tilde,
            ':' => {
                if self.cursor.eat_char('=') {
                    SimpleTokenKind::ColonEqual
                } else {
                    SimpleTokenKind::Colon
                }
            }
            ';' => SimpleTokenKind::Semi,
            '<' => {
                if self.cursor.eat_char('<') {
                    if self.cursor.eat_char('=') {
                        SimpleTokenKind::LeftShiftEqual
                    } else {
                        SimpleTokenKind::LeftShift
                    }
                } else if self.cursor.eat_char('=') {
                    SimpleTokenKind::LessEqual
                } else {
                    SimpleTokenKind::Less
                }
            }
            '>' => {
                if self.cursor.eat_char('>') {
                    if self.cursor.eat_char('=') {
                        SimpleTokenKind::RightShiftEqual
                    } else {
                        SimpleTokenKind::RightShift
                    }
                } else if self.cursor.eat_char('=') {
                    SimpleTokenKind::GreaterEqual
                } else {
                    SimpleTokenKind::Greater
                }
            }
            ',' => SimpleTokenKind::Comma,
            '.' => {
                if self.cursor.first() == '.' && self.cursor.second() == '.' {
                    self.cursor.bump();
                    self.cursor.bump();
                    SimpleTokenKind::Ellipsis
                } else {
                    SimpleTokenKind::Dot
                }
            }

            // Bracket tokens
            '(' => SimpleTokenKind::LParen,
            ')' => SimpleTokenKind::RParen,
            '[' => SimpleTokenKind::LBracket,
            ']' => SimpleTokenKind::RBracket,
            '{' => SimpleTokenKind::LBrace,
            '}' => SimpleTokenKind::RBrace,

            _ => {
                self.bogus = true;
                SimpleTokenKind::Other
            }
        }
    }

    pub fn skip_trivia(self) -> impl Iterator<Item = SimpleToken> + 'a {
        self.filter(|t| !t.kind().is_trivia())
    }
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

/// Simple zero allocation backwards tokenizer for finding preceding tokens.
///
/// The tokenizer must start at an offset that is trivia (e.g. not inside of a multiline string).
/// It will fail when reaching a string.
///
/// In case it finds something it can't parse, the tokenizer will return a
/// [`SimpleTokenKind::Other`] and then only a final [`SimpleTokenKind::Bogus`] afterwards.
pub struct BackwardsTokenizer<'a> {
    offset: TextSize,
    back_offset: TextSize,
    /// Not `&CommentRanges` to avoid a circular dependency.
    comment_ranges: &'a [TextRange],
    bogus: bool,
    source: &'a str,
    cursor: Cursor<'a>,
}

impl<'a> BackwardsTokenizer<'a> {
    pub fn new(source: &'a str, range: TextRange, comment_range: &'a [TextRange]) -> Self {
        Self {
            offset: range.start(),
            back_offset: range.end(),
            // Throw out any comments that follow the range.
            comment_ranges: &comment_range
                [..comment_range.partition_point(|comment| comment.start() <= range.end())],
            bogus: false,
            source,
            cursor: Cursor::new(&source[range]),
        }
    }

    pub fn up_to(offset: TextSize, source: &'a str, comment_range: &'a [TextRange]) -> Self {
        Self::new(source, TextRange::up_to(offset), comment_range)
    }

    pub fn skip_trivia(self) -> impl Iterator<Item = SimpleToken> + 'a {
        self.filter(|t| !t.kind().is_trivia())
    }

    pub fn next_token(&mut self) -> SimpleToken {
        self.cursor.start_token();
        self.back_offset = self.cursor.text_len() + self.offset;

        let Some(last) = self.cursor.bump_back() else {
            return SimpleToken {
                kind: SimpleTokenKind::EndOfFile,
                range: TextRange::empty(self.back_offset),
            };
        };

        if self.bogus {
            let token = SimpleToken {
                kind: SimpleTokenKind::Bogus,
                range: TextRange::up_to(self.back_offset),
            };

            // Set the cursor to EOF
            self.cursor = Cursor::new("");
            self.back_offset = TextSize::new(0);
            return token;
        }

        if let Some(comment) = self
            .comment_ranges
            .last()
            .filter(|comment| comment.contains_inclusive(self.back_offset))
        {
            self.comment_ranges = &self.comment_ranges[..self.comment_ranges.len() - 1];

            // Skip the comment without iterating over the chars manually.
            self.cursor = Cursor::new(&self.source[TextRange::new(self.offset, comment.start())]);
            debug_assert_eq!(self.cursor.text_len() + self.offset, comment.start());
            return SimpleToken {
                kind: SimpleTokenKind::Comment,
                range: comment.range(),
            };
        }

        let kind = match last {
            // Space, tab, or form feed. We ignore the true semantics of form feed, and treat it as
            // whitespace. Note that this will lex-out trailing whitespace from a comment as
            // whitespace rather than as part of the comment token, but this shouldn't matter for
            // our use case.
            ' ' | '\t' | '\x0C' => {
                self.cursor
                    .eat_back_while(|c| matches!(c, ' ' | '\t' | '\x0C'));
                SimpleTokenKind::Whitespace
            }

            '\r' => SimpleTokenKind::Newline,
            '\n' => {
                self.cursor.eat_char_back('\r');
                SimpleTokenKind::Newline
            }
            _ => self.next_token_inner(last),
        };

        let token_len = self.cursor.token_len();
        let start = self.back_offset - token_len;
        SimpleToken {
            kind,
            range: TextRange::at(start, token_len),
        }
    }

    /// Helper to parser the previous token once we skipped all whitespace
    fn next_token_inner(&mut self, last: char) -> SimpleTokenKind {
        match last {
            // Keywords and identifiers
            c if is_identifier_continuation(c) => {
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
                    to_keyword_or_other(&self.source[range])
                } else {
                    self.cursor = savepoint;
                    self.bogus = true;
                    SimpleTokenKind::Other
                }
            }

            // Non-trivia tokens that are unambiguous when lexing backwards.
            // In other words: these are characters that _don't_ appear at the
            // end of a multi-character token (like `!=`).
            '\\' => SimpleTokenKind::Continuation,
            ':' => SimpleTokenKind::Colon,
            '~' => SimpleTokenKind::Tilde,
            '%' => SimpleTokenKind::Percent,
            '|' => SimpleTokenKind::Vbar,
            ',' => SimpleTokenKind::Comma,
            ';' => SimpleTokenKind::Semi,
            '(' => SimpleTokenKind::LParen,
            ')' => SimpleTokenKind::RParen,
            '[' => SimpleTokenKind::LBracket,
            ']' => SimpleTokenKind::RBracket,
            '{' => SimpleTokenKind::LBrace,
            '}' => SimpleTokenKind::RBrace,
            '&' => SimpleTokenKind::Ampersand,
            '^' => SimpleTokenKind::Circumflex,
            '+' => SimpleTokenKind::Plus,
            '-' => SimpleTokenKind::Minus,

            // Non-trivia tokens that _are_ ambiguous when lexing backwards.
            // In other words: these are characters that _might_ mark the end
            // of a multi-character token (like `!=` or `->` or `//` or `**`).
            '=' | '*' | '/' | '@' | '!' | '<' | '>' | '.' => {
                // This could be a single-token token, like `+` in `x + y`, or a
                // multi-character token, like `+=` in `x += y`. It could also be a sequence
                // of multi-character tokens, like `x ==== y`, which is invalid, _but_ it's
                // important that we produce the same token stream when lexing backwards as
                // we do when lexing forwards. So, identify the range of the sequence, lex
                // forwards, and return the last token.
                let mut cursor = self.cursor.clone();
                cursor.eat_back_while(|c| {
                    matches!(
                        c,
                        ':' | '~'
                            | '%'
                            | '|'
                            | '&'
                            | '^'
                            | '+'
                            | '-'
                            | '='
                            | '*'
                            | '/'
                            | '@'
                            | '!'
                            | '<'
                            | '>'
                            | '.'
                    )
                });

                let token_len = cursor.token_len();
                let range = TextRange::at(self.back_offset - token_len, token_len);

                let forward_lexer = SimpleTokenizer::new(self.source, range);
                if let Some(token) = forward_lexer.last() {
                    // If the token spans multiple characters, bump the cursor. Note,
                    // though, that we already bumped the cursor to past the last character
                    // in the token at the very start of `next_token_back`.y
                    for _ in self.source[token.range].chars().rev().skip(1) {
                        self.cursor.bump_back().unwrap();
                    }
                    token.kind()
                } else {
                    self.bogus = true;
                    SimpleTokenKind::Other
                }
            }
            _ => {
                self.bogus = true;
                SimpleTokenKind::Other
            }
        }
    }
}

impl Iterator for BackwardsTokenizer<'_> {
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

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use ruff_python_parser::lexer::lex;
    use ruff_python_parser::{Mode, Tok};
    use ruff_text_size::{TextLen, TextRange, TextSize};

    use crate::tokenizer::{lines_after, lines_before, SimpleToken, SimpleTokenizer};
    use crate::{BackwardsTokenizer, SimpleTokenKind};

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
            let comment_ranges: Vec<_> = lex(self.source, Mode::Module)
                .filter_map(|result| {
                    let (token, range) = result.expect("Input to be a valid python program.");
                    if matches!(token, Tok::Comment(_)) {
                        Some(range)
                    } else {
                        None
                    }
                })
                .collect();
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
}
