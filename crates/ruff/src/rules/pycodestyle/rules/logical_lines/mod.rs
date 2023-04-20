use bitflags::bitflags;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;
use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;
use std::ops::Deref;

use ruff_python_ast::source_code::Locator;

pub(crate) use extraneous_whitespace::{
    extraneous_whitespace, WhitespaceAfterOpenBracket, WhitespaceBeforeCloseBracket,
    WhitespaceBeforePunctuation,
};
pub(crate) use indentation::{
    indentation, IndentationWithInvalidMultiple, IndentationWithInvalidMultipleComment,
    NoIndentedBlock, NoIndentedBlockComment, OverIndented, UnexpectedIndentation,
    UnexpectedIndentationComment,
};
pub(crate) use missing_whitespace::{missing_whitespace, MissingWhitespace};
pub(crate) use missing_whitespace_after_keyword::{
    missing_whitespace_after_keyword, MissingWhitespaceAfterKeyword,
};
pub(crate) use missing_whitespace_around_operator::{
    missing_whitespace_around_operator, MissingWhitespaceAroundArithmeticOperator,
    MissingWhitespaceAroundBitwiseOrShiftOperator, MissingWhitespaceAroundModuloOperator,
    MissingWhitespaceAroundOperator,
};
use ruff_python_ast::token_kind::{
    is_arithmetic_token, is_keyword_token, is_operator_token, is_unary_token,
};
pub(crate) use space_around_operator::{
    space_around_operator, MultipleSpacesAfterOperator, MultipleSpacesBeforeOperator,
    TabAfterOperator, TabBeforeOperator,
};
pub(crate) use whitespace_around_keywords::{
    whitespace_around_keywords, MultipleSpacesAfterKeyword, MultipleSpacesBeforeKeyword,
    TabAfterKeyword, TabBeforeKeyword,
};
pub(crate) use whitespace_around_named_parameter_equals::{
    whitespace_around_named_parameter_equals, MissingWhitespaceAroundParameterEquals,
    UnexpectedSpacesAroundKeywordParameterEquals,
};
pub(crate) use whitespace_before_comment::{
    whitespace_before_comment, MultipleLeadingHashesForBlockComment, NoSpaceAfterBlockComment,
    NoSpaceAfterInlineComment, TooFewSpacesBeforeInlineComment,
};
pub(crate) use whitespace_before_parameters::{
    whitespace_before_parameters, WhitespaceBeforeParameters,
};

mod extraneous_whitespace;
mod indentation;
mod missing_whitespace;
mod missing_whitespace_after_keyword;
mod missing_whitespace_around_operator;
mod space_around_operator;
mod whitespace_around_keywords;
mod whitespace_around_named_parameter_equals;
mod whitespace_before_comment;
mod whitespace_before_parameters;

bitflags! {
    #[derive(Default, Eq, PartialEq, Clone, Copy, Debug)]
    pub(crate) struct TokenFlags: u8 {
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

        /// Whether the logical line contains any non trivia token (no comment, newline, or in/dedent)
        const NON_TRIVIA = 0b0010_0000;
    }
}

impl TokenFlags {
    fn add_token(&mut self, token: &Tok) {
        if matches!(token, Tok::Comment { .. }) {
            self.insert(TokenFlags::COMMENT);
        } else if is_operator_token(token) {
            self.insert(TokenFlags::OPERATOR);

            self.set(
                TokenFlags::BRACKET,
                matches!(
                    token,
                    Tok::Lpar | Tok::Lsqb | Tok::Lbrace | Tok::Rpar | Tok::Rsqb | Tok::Rbrace
                ),
            );
        }

        if matches!(token, Tok::Comma | Tok::Semi | Tok::Colon) {
            self.insert(TokenFlags::PUNCTUATION);
        } else if is_keyword_token(token) {
            self.insert(TokenFlags::KEYWORD);
        }

        self.set(
            TokenFlags::NON_TRIVIA,
            !matches!(
                token,
                Tok::Comment { .. }
                    | Tok::Newline
                    | Tok::NonLogicalNewline
                    | Tok::Dedent
                    | Tok::Indent
            ),
        );
    }
}

#[derive(Clone)]
pub(crate) struct LogicalLinesIter<'a> {
    tokens: &'a [LexResult],
    locator: &'a Locator<'a>,
    parens: u32,
}

impl<'a> LogicalLinesIter<'a> {
    pub fn new(tokens: &'a [LexResult], locator: &'a Locator<'a>) -> Self {
        Self {
            tokens,
            locator,
            parens: 0,
        }
    }
}

impl<'a> Iterator for LogicalLinesIter<'a> {
    type Item = LogicalLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut flags = TokenFlags::empty();

        for (offset, token) in self.tokens.iter().enumerate() {
            let Ok((token, _)) = token else {
                // Skip the entire line
                return if let Some(newline_pos) = self.tokens.iter().position(|res| matches!(res, Ok((Tok::Newline | Tok::NonLogicalNewline | Tok::Comment( ..), _)))) {
                    self.tokens = &self.tokens[newline_pos + 1..];
                    self.next()
                } else {
                    self.tokens = &[];
                    None
                }
            };

            flags.add_token(token);

            match token {
                Tok::Lbrace | Tok::Lpar | Tok::Lsqb => {
                    self.parens += 1;
                }
                Tok::Rbrace | Tok::Rpar | Tok::Rsqb => {
                    self.parens -= 1;
                }
                Tok::Newline | Tok::NonLogicalNewline | Tok::Comment { .. } if self.parens == 0 => {
                    let (line_tokens, rest_tokens) = self.tokens.split_at(offset + 1);
                    self.tokens = rest_tokens;
                    return Some(LogicalLine {
                        flags,
                        tokens: line_tokens,
                        locator: self.locator,
                    });
                }
                _ => {}
            }
        }

        if self.tokens.is_empty() {
            None
        } else {
            Some(LogicalLine {
                flags,
                tokens: std::mem::take(&mut self.tokens),
                locator: self.locator,
            })
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
#[derive(Clone)]
pub(crate) struct LogicalLine<'a> {
    flags: TokenFlags,
    tokens: &'a [LexResult],
    locator: &'a Locator<'a>,
}

impl<'a> LogicalLine<'a> {
    /// Returns `true` if this is a comment only line
    pub fn is_comment_only(&self) -> bool {
        self.flags == TokenFlags::COMMENT
    }

    /// Returns logical line's text including comments, indents, dedent and trailing new lines.
    pub fn text(&self) -> &'a str {
        let tokens = self.tokens();
        match (self.first_token(), tokens.last()) {
            (Some(first), Some(last)) => self
                .locator
                .slice(TextRange::new(first.start(), last.end())),
            _ => "",
        }
    }

    /// Returns the text without any leading or trailing newline, comment, indent, or dedent of this line
    #[cfg(test)]
    pub fn text_trimmed(&self) -> &'a str {
        let mut trimmed = self.tokens_trimmed();
        let first = trimmed.next();
        let last = trimmed.next_back().or_else(|| first.clone());
        match (first, last) {
            (Some(first), Some(last)) => self
                .locator
                .slice(TextRange::new(first.start(), last.end())),
            _ => "",
        }
    }

    pub fn tokens_trimmed(&self) -> LogicalLineTokensIter<'a> {
        let start = self
            .tokens()
            .position(|t| {
                !matches!(
                    t.token(),
                    Tok::Newline
                        | Tok::NonLogicalNewline
                        | Tok::Indent
                        | Tok::Dedent
                        | Tok::Comment { .. },
                )
            })
            .unwrap_or(self.tokens.len());

        let end = self
            .tokens()
            .rposition(|t| {
                !matches!(
                    t.token(),
                    Tok::Newline
                        | Tok::NonLogicalNewline
                        | Tok::Indent
                        | Tok::Dedent
                        | Tok::Comment { .. },
                )
            })
            .map_or(start, |pos| pos + 1);

        LogicalLineTokensIter {
            inner: self.tokens[start..end].into_iter(),
        }
    }

    /// Returns the text after `token`
    #[inline]
    pub fn text_after(&self, token: &LogicalLineToken<'a>) -> &str {
        // SAFETY: The line must have at least one token or `token` would not belong to this line.
        let last_token = self.tokens().last().unwrap();
        self.locator
            .slice(TextRange::new(token.end(), last_token.end()))
    }

    /// Returns the text before `token`
    #[inline]
    pub fn text_before(&self, token: &LogicalLineToken<'a>) -> &str {
        // SAFETY: The line must have at least one token or `token` would not belong to this line.
        let first_token = self.tokens().next().unwrap();
        self.locator
            .slice(TextRange::new(first_token.start(), token.start()))
    }

    /// Returns the whitespace *after* the `token`
    pub fn trailing_whitespace(&self, token: &LogicalLineToken<'a>) -> Whitespace {
        Whitespace::leading(self.text_after(token))
    }

    /// Returns the whitespace and whitespace byte-length *before* the `token`
    pub fn leading_whitespace(&self, token: &LogicalLineToken<'a>) -> (Whitespace, TextSize) {
        Whitespace::trailing(self.text_before(token))
    }

    /// Returns all tokens of the line, including comments and trailing new lines.
    pub fn tokens(&self) -> LogicalLineTokensIter<'a> {
        LogicalLineTokensIter {
            inner: self.tokens.iter(),
        }
    }

    pub fn first_token(&self) -> Option<LogicalLineToken<'a>> {
        self.tokens().next()
    }

    pub fn last_token(&self) -> Option<LogicalLineToken<'a>> {
        self.tokens().next_back()
    }

    /// Returns the line's flags
    pub const fn flags(&self) -> TokenFlags {
        self.flags
    }
}

impl Debug for LogicalLine<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogicalLine")
            .field("text", &self.text())
            .field("flags", &self.flags())
            .field("tokens", &DebugLogicalLines(self))
            .finish()
    }
}

struct DebugLogicalLines<'a>(&'a LogicalLine<'a>);

impl Debug for DebugLogicalLines<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.tokens()).finish()
    }
}

pub(crate) struct LogicalLineTokensIter<'a> {
    inner: std::slice::Iter<'a, LexResult>,
}

impl<'a> Iterator for LogicalLineTokensIter<'a> {
    type Item = LogicalLineToken<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.inner.next()?;

        // SAFETY: Guaranteed to be `OK` because the `LogicalLinesIter` aborts on the first `Err` token.
        #[allow(unsafe_code)]
        let spanned = unsafe { result.as_ref().unwrap_unchecked() };
        Some(LogicalLineToken { spanned })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for LogicalLineTokensIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let result = self.inner.next_back()?;
        // SAFETY: Guaranteed to be `OK` because the `LogicalLinesIter` aborts on the first `Err` token.
        #[allow(unsafe_code)]
        let spanned = unsafe { result.as_ref().unwrap_unchecked() };
        Some(LogicalLineToken { spanned })
    }
}

impl ExactSizeIterator for LogicalLineTokensIter<'_> {}
impl FusedIterator for LogicalLineTokensIter<'_> {}

/// A token of a [`LogicalLine`]
#[derive(Clone, Debug)]
pub(crate) struct LogicalLineToken<'a> {
    spanned: &'a (Tok, TextRange),
}

impl<'a> LogicalLineToken<'a> {
    pub const fn token(&self) -> &'a Tok {
        &self.spanned.0
    }

    /// Returns the token's start location
    #[inline]
    pub const fn start(&self) -> TextSize {
        self.range().start()
    }

    /// Returns the token's end location
    #[inline]
    pub const fn end(&self) -> TextSize {
        self.range().end()
    }

    /// Returns a tuple with the token's `(start, end)` locations
    #[inline]
    pub const fn range(&self) -> TextRange {
        self.spanned.1
    }

    pub const fn is_whitespace_needed(&self) -> bool {
        matches!(
            self.token(),
            Tok::DoubleStarEqual
                | Tok::StarEqual
                | Tok::SlashEqual
                | Tok::DoubleSlashEqual
                | Tok::PlusEqual
                | Tok::MinusEqual
                | Tok::NotEqual
                | Tok::Less
                | Tok::Greater
                | Tok::PercentEqual
                | Tok::CircumflexEqual
                | Tok::AmperEqual
                | Tok::VbarEqual
                | Tok::EqEqual
                | Tok::LessEqual
                | Tok::GreaterEqual
                | Tok::LeftShiftEqual
                | Tok::RightShiftEqual
                | Tok::Equal
                | Tok::And
                | Tok::Or
                | Tok::In
                | Tok::Is
                | Tok::Rarrow
        )
    }

    #[inline]
    pub const fn is_whitespace_optional(&self) -> bool {
        let token = self.token();
        is_arithmetic_token(token)
            || matches!(
                token,
                Tok::CircumFlex
                    | Tok::Amper
                    | Tok::Vbar
                    | Tok::LeftShift
                    | Tok::RightShift
                    | Tok::Percent
            )
    }

    pub const fn is_indent(&self) -> bool {
        matches!(self.token(), Tok::Indent)
    }

    pub const fn is_colon(&self) -> bool {
        matches!(self.token(), Tok::Colon)
    }

    pub const fn is_except(&self) -> bool {
        matches!(self.token(), Tok::Except)
    }

    pub const fn is_star(&self) -> bool {
        matches!(self.token(), Tok::Star)
    }

    pub const fn is_yield(&self) -> bool {
        matches!(self.token(), Tok::Yield)
    }

    pub const fn is_rpar(&self) -> bool {
        matches!(self.token(), Tok::Rpar)
    }

    pub const fn is_greater(&self) -> bool {
        matches!(self.token(), Tok::Greater)
    }

    pub const fn is_operator(&self) -> bool {
        is_operator_token(self.token())
    }

    pub const fn is_name(&self) -> bool {
        matches!(self.token(), Tok::Name { .. })
    }

    pub const fn is_equal(&self) -> bool {
        matches!(self.token(), Tok::Equal)
    }

    pub const fn is_comment(&self) -> bool {
        matches!(self.token(), Tok::Comment(..))
    }

    pub const fn is_unary(&self) -> bool {
        is_unary_token(self.token())
    }

    pub const fn is_keyword(&self) -> bool {
        is_keyword_token(self.token())
    }

    #[inline]
    pub const fn is_skip_comment(&self) -> bool {
        matches!(
            self.token(),
            Tok::Newline | Tok::Indent | Tok::Dedent | Tok::NonLogicalNewline | Tok::Comment { .. }
        )
    }

    pub const fn is_non_logical_newline(&self) -> bool {
        matches!(self.token(), Tok::NonLogicalNewline)
    }
}

impl Deref for LogicalLineToken<'_> {
    type Target = Tok;

    fn deref(&self) -> &Self::Target {
        self.token()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum Whitespace {
    None,
    Single,
    Many,
    Tab,
}

impl Whitespace {
    const fn is_none(self) -> bool {
        matches!(self, Whitespace::None)
    }

    fn leading(content: &str) -> Self {
        let mut count = 0u32;

        for c in content.chars() {
            if c == '\t' {
                return Self::Tab;
            } else if matches!(c, '\n' | '\r') {
                break;
            } else if c.is_whitespace() {
                count += 1;
            } else {
                break;
            }
        }

        match count {
            0 => Whitespace::None,
            1 => Whitespace::Single,
            _ => Whitespace::Many,
        }
    }

    fn trailing(content: &str) -> (Self, TextSize) {
        let mut len = TextSize::default();
        let mut count = 0usize;

        for c in content.chars().rev() {
            if c == '\t' {
                return (Self::Tab, len + c.text_len());
            } else if matches!(c, '\n' | '\r') {
                // Indent
                return (Self::None, TextSize::default());
            } else if c.is_whitespace() {
                count += 1;
                len += c.text_len();
            } else {
                break;
            }
        }

        match count {
            0 => (Self::None, TextSize::default()),
            1 => (Self::Single, len),
            _ => {
                if len == content.text_len() {
                    // All whitespace up to the start of the line -> Indent
                    (Self::None, TextSize::default())
                } else {
                    (Self::Many, len)
                }
            }
        }
    }
}
