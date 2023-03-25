use bitflags::bitflags;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;
use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;

use ruff_python_ast::source_code::Locator;
use ruff_python_ast::token_kind::TokenKind;
use ruff_python_ast::types::Range;

bitflags! {
    #[derive(Default)]
    pub struct TokenFlags: u8 {
        /// Whether the logical line contains an operator.
        const OPERATOR = 0b0000_0001;
        /// Whether the logical line contains a bracket.
        const BRACKET = 0b0000_0010;
        /// Whether the logical line contains a punctuation mark.
        const PUNCTUATION = 0b0000_0100;
        /// Whether the logical line contains a keyword.
        const KEYWORD = 0b0000_1000;
        /// Whether the logical line contains a comment.
        const COMMENT = 0b0001_0000;
    }
}

#[derive(Clone)]
pub struct LogicalLines<'a> {
    tokens: Tokens,
    lines: Vec<Line>,
    locator: &'a Locator<'a>,
}

impl<'a> LogicalLines<'a> {
    pub fn from_tokens(tokens: &[LexResult], locator: &'a Locator<'a>) -> Self {
        assert!(u32::try_from(tokens.len()).is_ok());

        let mut builder = LogicalLinesBuilder::with_capacity(tokens.len());
        let mut parens: u32 = 0;

        for (start, token, end) in tokens.iter().flatten() {
            let token_kind = TokenKind::from_token(token);
            builder.push_token(*start, token, *end);

            match token_kind {
                TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                    parens += 1;
                }
                TokenKind::Rbrace | TokenKind::Rpar | TokenKind::Rsqb => {
                    parens -= 1;
                }
                TokenKind::Newline | TokenKind::NonLogicalNewline | TokenKind::Comment
                    if parens == 0 =>
                {
                    builder.finish_line();
                }
                _ => {}
            }
        }

        builder.finish(locator)
    }
}

impl Debug for LogicalLines<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.into_iter().map(DebugLogicalLine))
            .finish()
    }
}

impl<'a> IntoIterator for &'a LogicalLines<'a> {
    type Item = LogicalLine<'a>;
    type IntoIter = LogicalLinesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        LogicalLinesIter {
            lines: self,
            inner: self.lines.iter(),
        }
    }
}

/// A logical line spawns multiple lines in the source document if the line
/// ends with a parenthesized expression (`(..)`, `[..]`, `{..}`) that contains
/// line breaks.
///
/// ## Examples
/// This expression forms one logical line because because the array elements are parenthesized.
///
/// ```python
/// a = [
///     1,
///     2
/// ]
/// ```
#[derive(Debug)]
pub struct LogicalLine<'a> {
    lines: &'a LogicalLines<'a>,
    line: &'a Line,
}

impl<'a> LogicalLine<'a> {
    /// Returns `true` if this is a comment only line
    pub fn is_comment_only(&self) -> bool {
        self.flags() == TokenFlags::COMMENT && self.tokens().trimmed().is_empty()
    }

    /// Returns logical line's text including comments, indents, dedent and trailing new lines.
    pub fn text(&self) -> &'a str {
        self.tokens().text()
    }

    /// Returns the text without any leading or trailing newline, comment, indent, or dedent of this line
    pub fn text_trimmed(&self) -> &'a str {
        self.tokens().trimmed().text()
    }

    pub fn text_after(&self, token: &LogicalLineToken<'a>) -> &str {
        debug_assert!(
            (self.line.tokens_start..self.line.tokens_end).contains(&token.position),
            "Token does not belong to this line"
        );

        // SAFETY: The line must have at least one token or `token` would not belong to this line.
        let last_token = self.tokens().last().unwrap();
        self.lines
            .locator
            .slice(Range::new(token.end(), last_token.end()))
    }

    pub fn text_before(&self, token: &LogicalLineToken<'a>) -> &str {
        debug_assert!(
            (self.line.tokens_start..self.line.tokens_end).contains(&token.position),
            "Token does not belong to this line"
        );

        // SAFETY: The line must have at least one token or `token` would not belong to this line.
        let first_token = self.tokens().first().unwrap();
        self.lines
            .locator
            .slice(Range::new(first_token.start(), token.start()))
    }

    pub fn tokens_trimmed(&self) -> LogicalLineTokens<'a> {
        self.tokens().trimmed()
    }

    /// Returns all tokens of the line, including comments and trailing new lines.
    pub fn tokens(&self) -> LogicalLineTokens<'a> {
        LogicalLineTokens {
            lines: self.lines,
            front: self.line.tokens_start,
            back: self.line.tokens_end,
        }
    }

    /// Returns the [`Location`] of the first token on the line or [`None`].
    pub fn first_token_location(&self) -> Option<Location> {
        self.tokens().first().map(|t| t.start())
    }

    /// Returns the line's flags
    pub const fn flags(&self) -> TokenFlags {
        self.line.flags
    }
}

/// Helper struct to pretty print [`LogicalLine`] with `dbg`
struct DebugLogicalLine<'a>(LogicalLine<'a>);

impl Debug for DebugLogicalLine<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogicalLine")
            .field("text", &self.0.text())
            .field("flags", &self.0.flags())
            .field("tokens", &self.0.tokens())
            .finish()
    }
}

/// Iterator over the logical lines of a document.
pub struct LogicalLinesIter<'a> {
    lines: &'a LogicalLines<'a>,
    inner: std::slice::Iter<'a, Line>,
}

impl<'a> Iterator for LogicalLinesIter<'a> {
    type Item = LogicalLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let line = self.inner.next()?;

        Some(LogicalLine {
            lines: self.lines,
            line,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for LogicalLinesIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let line = self.inner.next_back()?;

        Some(LogicalLine {
            lines: self.lines,
            line,
        })
    }
}

impl ExactSizeIterator for LogicalLinesIter<'_> {}

impl FusedIterator for LogicalLinesIter<'_> {}

/// The tokens of a logical line
pub struct LogicalLineTokens<'a> {
    lines: &'a LogicalLines<'a>,
    front: u32,
    back: u32,
}

impl<'a> LogicalLineTokens<'a> {
    pub fn iter(&self) -> LogicalLineTokensIter<'a> {
        LogicalLineTokensIter {
            tokens: &self.lines.tokens,
            front: self.front,
            back: self.back,
        }
    }

    pub fn len(&self) -> usize {
        (self.back - self.front) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn trimmed(&self) -> LogicalLineTokens<'a> {
        let kinds = &self.lines.tokens.kinds[self.front as usize..self.back as usize];
        let mut front = self.front;
        let mut back = self.back;

        let mut iter = kinds.iter();

        for kind in iter.by_ref() {
            if !matches!(
                kind,
                TokenKind::Newline
                    | TokenKind::NonLogicalNewline
                    | TokenKind::Indent
                    | TokenKind::Dedent
                    | TokenKind::Comment
            ) {
                break;
            }
            front += 1;
        }

        for kind in iter.rev() {
            if !matches!(
                kind,
                TokenKind::Newline
                    | TokenKind::NonLogicalNewline
                    | TokenKind::Indent
                    | TokenKind::Dedent
                    | TokenKind::Comment
            ) {
                break;
            }
            back -= 1;
        }

        LogicalLineTokens {
            lines: self.lines,
            front,
            back,
        }
    }

    pub fn text(&self) -> &'a str {
        match (self.first(), self.last()) {
            (Some(first), Some(last)) => {
                let locator = self.lines.locator;
                locator.slice(Range::new(first.start(), last.end()))
            }
            _ => "",
        }
    }

    /// Returns the first token
    pub fn first(&self) -> Option<LogicalLineToken<'a>> {
        self.iter().next()
    }

    /// Returns the last token
    pub fn last(&self) -> Option<LogicalLineToken<'a>> {
        self.iter().next_back()
    }
}

impl<'a> IntoIterator for LogicalLineTokens<'a> {
    type Item = LogicalLineToken<'a>;
    type IntoIter = LogicalLineTokensIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &LogicalLineTokens<'a> {
    type Item = LogicalLineToken<'a>;
    type IntoIter = LogicalLineTokensIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Debug for LogicalLineTokens<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Iterator over the tokens of a [`LogicalLine`]
pub struct LogicalLineTokensIter<'a> {
    tokens: &'a Tokens,
    front: u32,
    back: u32,
}

impl<'a> Iterator for LogicalLineTokensIter<'a> {
    type Item = LogicalLineToken<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front < self.back {
            let result = Some(LogicalLineToken {
                tokens: self.tokens,
                position: self.front,
            });

            self.front += 1;
            result
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.back - self.front) as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for LogicalLineTokensIter<'_> {}

impl FusedIterator for LogicalLineTokensIter<'_> {}

impl DoubleEndedIterator for LogicalLineTokensIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front < self.back {
            self.back -= 1;
            Some(LogicalLineToken {
                position: self.back,
                tokens: self.tokens,
            })
        } else {
            None
        }
    }
}

/// A token of a [`LogicalLine`]
#[derive(Clone)]
pub struct LogicalLineToken<'a> {
    tokens: &'a Tokens,
    position: u32,
}

impl<'a> LogicalLineToken<'a> {
    /// Returns the token's kind
    pub fn kind(&self) -> TokenKind {
        #[allow(unsafe_code)]
        unsafe {
            *self.tokens.kinds.get_unchecked(self.position as usize)
        }
    }

    /// Returns the token's start location
    pub fn start(&self) -> Location {
        self.range().0
    }

    /// Returns the token's end location
    pub fn end(&self) -> Location {
        self.range().1
    }

    /// Returns a tuple with the token's `(start, end)` locations
    pub fn range(&self) -> (Location, Location) {
        #[allow(unsafe_code)]
        let &(start, end) = unsafe { self.tokens.locations.get_unchecked(self.position as usize) };

        (start, end)
    }
}

impl Debug for LogicalLineToken<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogicalLineToken")
            .field("kind", &self.kind())
            .field("range", &self.range())
            .finish()
    }
}

#[derive(Debug, Default)]
struct CurrentLine {
    flags: TokenFlags,
    tokens_start: u32,
}

/// Builder for [`LogicalLines`]
#[derive(Debug, Default)]
struct LogicalLinesBuilder {
    tokens: Tokens,
    lines: Vec<Line>,
    current_line: Option<CurrentLine>,
}

impl LogicalLinesBuilder {
    fn with_capacity(tokens: usize) -> Self {
        Self {
            tokens: Tokens::with_capacity(tokens),
            ..Self::default()
        }
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    #[allow(clippy::cast_possible_truncation)]
    fn push_token(&mut self, start: Location, token: &Tok, end: Location) {
        let tokens_start = self.tokens.len();
        let token_kind = TokenKind::from_token(token);

        let line = self.current_line.get_or_insert_with(|| CurrentLine {
            flags: TokenFlags::empty(),
            tokens_start: tokens_start as u32,
        });

        if matches!(token_kind, TokenKind::Comment) {
            line.flags.insert(TokenFlags::COMMENT);
        } else if token_kind.is_operator() {
            line.flags.insert(TokenFlags::OPERATOR);

            line.flags.set(
                TokenFlags::BRACKET,
                matches!(
                    token_kind,
                    TokenKind::Lpar
                        | TokenKind::Lsqb
                        | TokenKind::Lbrace
                        | TokenKind::Rpar
                        | TokenKind::Rsqb
                        | TokenKind::Rbrace
                ),
            );
        }

        if matches!(
            token_kind,
            TokenKind::Comma | TokenKind::Semi | TokenKind::Colon
        ) {
            line.flags.insert(TokenFlags::PUNCTUATION);
        } else if token_kind.is_keyword() {
            line.flags.insert(TokenFlags::KEYWORD);
        }

        self.tokens.push(token_kind, start, end);
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    #[allow(clippy::cast_possible_truncation)]
    fn finish_line(&mut self) {
        if let Some(current) = self.current_line.take() {
            self.lines.push(Line {
                flags: current.flags,
                tokens_start: current.tokens_start,
                tokens_end: self.tokens.len() as u32,
            });
        }
    }

    fn finish<'a>(mut self, locator: &'a Locator<'a>) -> LogicalLines<'a> {
        self.finish_line();

        LogicalLines {
            tokens: self.tokens,
            lines: self.lines,
            locator,
        }
    }
}

#[derive(Debug, Clone)]
struct Line {
    flags: TokenFlags,
    tokens_start: u32,
    tokens_end: u32,
}

#[derive(Debug, Clone, Default)]
struct Tokens {
    /// Stores the kinds in a separate vec because most checkers scan for a specific kind.
    /// This speeds up scanning because it avoids loading the start, end locations into the L1 cache.
    kinds: Vec<TokenKind>,
    locations: Vec<(Location, Location)>,
}

impl Tokens {
    /// Creates new tokens with a reserved size of `capacity`
    fn with_capacity(capacity: usize) -> Self {
        Self {
            kinds: Vec::with_capacity(capacity),
            locations: Vec::with_capacity(capacity),
        }
    }

    /// Returns the number of stored tokens.
    fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Adds a new token with the given `kind` and `start`, `end` location.
    fn push(&mut self, kind: TokenKind, start: Location, end: Location) {
        self.kinds.push(kind);
        self.locations.push((start, end));
    }
}
