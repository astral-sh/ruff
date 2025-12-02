use std::{iter::FusedIterator, ops::Deref};

use super::{Token, TokenKind};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::{Ranged as _, TextRange, TextSize};

/// Tokens represents a vector of lexed [`Token`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
pub struct Tokens {
    raw: Vec<Token>,
}

impl Tokens {
    pub fn new(tokens: Vec<Token>) -> Tokens {
        Tokens { raw: tokens }
    }

    /// Returns an iterator over all the tokens that provides context.
    pub fn iter_with_context(&self) -> TokenIterWithContext<'_> {
        TokenIterWithContext::new(&self.raw)
    }

    /// Performs a binary search to find the index of the **first** token that starts at the given `offset`.
    ///
    /// Unlike `binary_search_by_key`, this method ensures that if multiple tokens start at the same offset,
    /// it returns the index of the first one. Multiple tokens can start at the same offset in cases where
    /// zero-length tokens are involved (like `Dedent` or `Newline` at the end of the file).
    pub fn binary_search_by_start(&self, offset: TextSize) -> Result<usize, usize> {
        let partition_point = self.partition_point(|token| token.start() < offset);

        let after = &self[partition_point..];

        if after.first().is_some_and(|first| first.start() == offset) {
            Ok(partition_point)
        } else {
            Err(partition_point)
        }
    }

    /// Returns a slice of [`Token`] that are within the given `range`.
    ///
    /// The start and end offset of the given range should be either:
    /// 1. Token boundary
    /// 2. Gap between the tokens
    ///
    /// For example, considering the following tokens and their corresponding range:
    ///
    /// | Token               | Range     |
    /// |---------------------|-----------|
    /// | `Def`               | `0..3`    |
    /// | `Name`              | `4..7`    |
    /// | `Lpar`              | `7..8`    |
    /// | `Rpar`              | `8..9`    |
    /// | `Colon`             | `9..10`   |
    /// | `Newline`           | `10..11`  |
    /// | `Comment`           | `15..24`  |
    /// | `NonLogicalNewline` | `24..25`  |
    /// | `Indent`            | `25..29`  |
    /// | `Pass`              | `29..33`  |
    ///
    /// Here, for (1) a token boundary is considered either the start or end offset of any of the
    /// above tokens. For (2), the gap would be any offset between the `Newline` and `Comment`
    /// token which are 12, 13, and 14.
    ///
    /// Examples:
    /// 1) `4..10` would give `Name`, `Lpar`, `Rpar`, `Colon`
    /// 2) `11..25` would give `Comment`, `NonLogicalNewline`
    /// 3) `12..25` would give same as (2) and offset 12 is in the "gap"
    /// 4) `9..12` would give `Colon`, `Newline` and offset 12 is in the "gap"
    /// 5) `18..27` would panic because both the start and end offset is within a token
    ///
    /// ## Note
    ///
    /// The returned slice can contain the [`TokenKind::Unknown`] token if there was a lexical
    /// error encountered within the given range.
    ///
    /// # Panics
    ///
    /// If either the start or end offset of the given range is within a token range.
    pub fn in_range(&self, range: TextRange) -> &[Token] {
        let tokens_after_start = self.after(range.start());

        Self::before_impl(tokens_after_start, range.end())
    }

    /// Searches the token(s) at `offset`.
    ///
    /// Returns [`TokenAt::Between`] if `offset` points directly inbetween two tokens
    /// (the left token ends at `offset` and the right token starts at `offset`).
    ///
    ///
    /// ## Examples
    ///
    /// [Playground](https://play.ruff.rs/f3ad0a55-5931-4a13-96c7-b2b8bfdc9a2e?secondary=Tokens)
    ///
    /// ```
    /// # use ruff_python_ast::PySourceType;
    /// # use ruff_python_parser::{Token, TokenAt, TokenKind};
    /// # use ruff_text_size::{Ranged, TextSize};
    ///
    /// let source = r#"
    /// def test(arg):
    ///     arg.call()
    ///     if True:
    ///         pass
    ///     print("true")
    /// "#.trim();
    ///
    /// let parsed = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    /// let tokens = parsed.tokens();
    ///
    /// let collect_tokens = |offset: TextSize| {
    ///     tokens.at_offset(offset).into_iter().map(|t| (t.kind(), &source[t.range()])).collect::<Vec<_>>()
    /// };
    ///
    /// assert_eq!(collect_tokens(TextSize::new(4)), vec! [(TokenKind::Name, "test")]);
    /// assert_eq!(collect_tokens(TextSize::new(6)), vec! [(TokenKind::Name, "test")]);
    /// // between `arg` and `.`
    /// assert_eq!(collect_tokens(TextSize::new(22)), vec! [(TokenKind::Name, "arg"), (TokenKind::Dot, ".")]);
    /// assert_eq!(collect_tokens(TextSize::new(36)), vec! [(TokenKind::If, "if")]);
    /// // Before the dedent token
    /// assert_eq!(collect_tokens(TextSize::new(57)), vec! []);
    /// ```
    pub fn at_offset(&self, offset: TextSize) -> TokenAt {
        match self.binary_search_by_start(offset) {
            // The token at `index` starts exactly at `offset.
            // ```python
            // object.attribute
            //        ^ OFFSET
            // ```
            Ok(index) => {
                let token = self[index];
                // `token` starts exactly at `offset`. Test if the offset is right between
                // `token` and the previous token (if there's any)
                if let Some(previous) = index.checked_sub(1).map(|idx| self[idx]) {
                    if previous.end() == offset {
                        return TokenAt::Between(previous, token);
                    }
                }

                TokenAt::Single(token)
            }

            // No token found that starts exactly at the given offset. But it's possible that
            // the token starting before `offset` fully encloses `offset` (it's end range ends after `offset`).
            // ```python
            // object.attribute
            //   ^ OFFSET
            // # or
            // if True:
            //     print("test")
            //  ^ OFFSET
            // ```
            Err(index) => {
                if let Some(previous) = index.checked_sub(1).map(|idx| self[idx]) {
                    if previous.range().contains_inclusive(offset) {
                        return TokenAt::Single(previous);
                    }
                }

                TokenAt::None
            }
        }
    }

    /// Returns a slice of tokens before the given [`TextSize`] offset.
    ///
    /// If the given offset is between two tokens, the returned slice will end just before the
    /// following token. In other words, if the offset is between the end of previous token and
    /// start of next token, the returned slice will end just before the next token.
    ///
    /// # Panics
    ///
    /// If the given offset is inside a token range at any point
    /// other than the start of the range.
    pub fn before(&self, offset: TextSize) -> &[Token] {
        Self::before_impl(&self.raw, offset)
    }

    fn before_impl(tokens: &[Token], offset: TextSize) -> &[Token] {
        let partition_point = tokens.partition_point(|token| token.start() < offset);
        let before = &tokens[..partition_point];

        if let Some(last) = before.last() {
            // If it's equal to the end offset, then it's at a token boundary which is
            // valid. If it's greater than the end offset, then it's in the gap between
            // the tokens which is valid as well.
            assert!(
                offset >= last.end(),
                "Offset {:?} is inside a token range {:?}",
                offset,
                last.range()
            );
        }
        before
    }

    /// Returns a slice of tokens after the given [`TextSize`] offset.
    ///
    /// If the given offset is between two tokens, the returned slice will start from the following
    /// token. In other words, if the offset is between the end of previous token and start of next
    /// token, the returned slice will start from the next token.
    ///
    /// # Panics
    ///
    /// If the given offset is inside a token range at any point
    /// other than the start of the range.
    pub fn after(&self, offset: TextSize) -> &[Token] {
        let partition_point = self.partition_point(|token| token.end() <= offset);
        let after = &self[partition_point..];

        if let Some(first) = after.first() {
            // valid. If it's greater than the end offset, then it's in the gap between
            // the tokens which is valid as well.
            assert!(
                offset <= first.start(),
                "Offset {:?} is inside a token range {:?}",
                offset,
                first.range()
            );
        }

        after
    }
}

impl<'a> IntoIterator for &'a Tokens {
    type Item = &'a Token;
    type IntoIter = std::slice::Iter<'a, Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Deref for Tokens {
    type Target = [Token];

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

/// A token that encloses a given offset or ends exactly at it.
#[derive(Debug, Clone)]
pub enum TokenAt {
    /// There's no token at the given offset
    None,

    /// There's a single token at the given offset.
    Single(Token),

    /// The offset falls exactly between two tokens. E.g. `CURSOR` in `call<CURSOR>(arguments)` is
    /// positioned exactly between the `call` and `(` tokens.
    Between(Token, Token),
}

impl Iterator for TokenAt {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        match *self {
            TokenAt::None => None,
            TokenAt::Single(token) => {
                *self = TokenAt::None;
                Some(token)
            }
            TokenAt::Between(first, second) => {
                *self = TokenAt::Single(second);
                Some(first)
            }
        }
    }
}

impl FusedIterator for TokenAt {}

impl From<&Tokens> for CommentRanges {
    fn from(tokens: &Tokens) -> Self {
        let mut ranges = vec![];
        for token in tokens {
            if token.kind() == TokenKind::Comment {
                ranges.push(token.range());
            }
        }
        CommentRanges::new(ranges)
    }
}

/// An iterator over the [`Token`]s with context.
///
/// This struct is created by the [`iter_with_context`] method on [`Tokens`]. Refer to its
/// documentation for more details.
///
/// [`iter_with_context`]: Tokens::iter_with_context
#[derive(Debug, Clone)]
pub struct TokenIterWithContext<'a> {
    inner: std::slice::Iter<'a, Token>,
    nesting: u32,
}

impl<'a> TokenIterWithContext<'a> {
    fn new(tokens: &'a [Token]) -> TokenIterWithContext<'a> {
        TokenIterWithContext {
            inner: tokens.iter(),
            nesting: 0,
        }
    }

    /// Return the nesting level the iterator is currently in.
    pub const fn nesting(&self) -> u32 {
        self.nesting
    }

    /// Returns `true` if the iterator is within a parenthesized context.
    pub const fn in_parenthesized_context(&self) -> bool {
        self.nesting > 0
    }

    /// Returns the next [`Token`] in the iterator without consuming it.
    pub fn peek(&self) -> Option<&'a Token> {
        self.clone().next()
    }
}

impl<'a> Iterator for TokenIterWithContext<'a> {
    type Item = &'a Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.inner.next()?;

        match token.kind() {
            TokenKind::Lpar | TokenKind::Lbrace | TokenKind::Lsqb => self.nesting += 1,
            TokenKind::Rpar | TokenKind::Rbrace | TokenKind::Rsqb => {
                self.nesting = self.nesting.saturating_sub(1);
            }
            // This mimics the behavior of re-lexing which reduces the nesting level on the lexer.
            // We don't need to reduce it by 1 because unlike the lexer we see the final token
            // after recovering from every unclosed parenthesis.
            TokenKind::Newline if self.nesting > 0 => {
                self.nesting = 0;
            }
            _ => {}
        }

        Some(token)
    }
}

impl FusedIterator for TokenIterWithContext<'_> {}
