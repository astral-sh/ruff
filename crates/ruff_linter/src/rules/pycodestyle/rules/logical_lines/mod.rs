pub(crate) use extraneous_whitespace::*;
pub(crate) use indentation::*;
pub(crate) use missing_whitespace::*;
pub(crate) use missing_whitespace_after_keyword::*;
pub(crate) use missing_whitespace_around_operator::*;
pub(crate) use space_around_operator::*;
pub(crate) use whitespace_around_keywords::*;
pub(crate) use whitespace_around_named_parameter_equals::*;
pub(crate) use whitespace_before_comment::*;
pub(crate) use whitespace_before_parameters::*;

use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;

use bitflags::bitflags;
use ruff_python_parser::lexer::LexResult;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use ruff_python_parser::TokenKind;
use ruff_python_trivia::is_python_whitespace;
use ruff_source_file::Locator;

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

#[derive(Clone)]
pub(crate) struct LogicalLines<'a> {
    tokens: Vec<LogicalLineToken>,
    lines: Vec<Line>,
    locator: &'a Locator<'a>,
}

impl<'a> LogicalLines<'a> {
    pub(crate) fn from_tokens(tokens: &'a [LexResult], locator: &'a Locator<'a>) -> Self {
        assert!(u32::try_from(tokens.len()).is_ok());

        let mut builder = LogicalLinesBuilder::with_capacity(tokens.len());
        let mut parens = 0u32;

        for (token, range) in tokens.iter().flatten() {
            let token_kind = TokenKind::from_token(token);
            builder.push_token(token_kind, *range);

            match token_kind {
                TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                    parens = parens.saturating_add(1);
                }
                TokenKind::Rbrace | TokenKind::Rpar | TokenKind::Rsqb => {
                    parens = parens.saturating_sub(1);
                }
                TokenKind::Newline | TokenKind::NonLogicalNewline if parens == 0 => {
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
    type IntoIter = LogicalLinesIter<'a>;
    type Item = LogicalLine<'a>;

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
pub(crate) struct LogicalLine<'a> {
    lines: &'a LogicalLines<'a>,
    line: &'a Line,
}

impl<'a> LogicalLine<'a> {
    /// Returns `true` if this line is positioned at the start of the file.
    pub(crate) const fn is_start_of_file(&self) -> bool {
        self.line.tokens_start == 0
    }

    /// Returns `true` if this is a comment only line
    pub(crate) fn is_comment_only(&self) -> bool {
        self.flags() == TokenFlags::COMMENT
    }

    /// Returns logical line's text including comments, indents, dedent and trailing new lines.
    pub(crate) fn text(&self) -> &'a str {
        let tokens = self.tokens();
        match (tokens.first(), tokens.last()) {
            (Some(first), Some(last)) => self
                .lines
                .locator
                .slice(TextRange::new(first.start(), last.end())),
            _ => "",
        }
    }

    /// Returns the text without any leading or trailing newline, comment, indent, or dedent of this line
    #[cfg(test)]
    pub(crate) fn text_trimmed(&self) -> &'a str {
        let tokens = self.tokens_trimmed();

        match (tokens.first(), tokens.last()) {
            (Some(first), Some(last)) => self
                .lines
                .locator
                .slice(TextRange::new(first.start(), last.end())),
            _ => "",
        }
    }

    pub(crate) fn tokens_trimmed(&self) -> &'a [LogicalLineToken] {
        let tokens = self.tokens();

        let start = tokens
            .iter()
            .position(|t| {
                !matches!(
                    t.kind(),
                    TokenKind::Newline
                        | TokenKind::NonLogicalNewline
                        | TokenKind::Indent
                        | TokenKind::Dedent
                        | TokenKind::Comment,
                )
            })
            .unwrap_or(tokens.len());

        let tokens = &tokens[start..];

        let end = tokens
            .iter()
            .rposition(|t| {
                !matches!(
                    t.kind(),
                    TokenKind::Newline
                        | TokenKind::NonLogicalNewline
                        | TokenKind::Indent
                        | TokenKind::Dedent
                        | TokenKind::Comment,
                )
            })
            .map_or(0, |pos| pos + 1);

        &tokens[..end]
    }

    /// Returns the text after `token`
    #[inline]
    pub(crate) fn text_after(&self, token: &'a LogicalLineToken) -> &str {
        // SAFETY: The line must have at least one token or `token` would not belong to this line.
        let last_token = self.tokens().last().unwrap();
        self.lines
            .locator
            .slice(TextRange::new(token.end(), last_token.end()))
    }

    /// Returns the text before `token`
    #[inline]
    pub(crate) fn text_before(&self, token: &'a LogicalLineToken) -> &str {
        // SAFETY: The line must have at least one token or `token` would not belong to this line.
        let first_token = self.tokens().first().unwrap();
        self.lines
            .locator
            .slice(TextRange::new(first_token.start(), token.start()))
    }

    /// Returns the whitespace *after* the `token` with the byte length
    pub(crate) fn trailing_whitespace(
        &self,
        token: &'a LogicalLineToken,
    ) -> (Whitespace, TextSize) {
        Whitespace::leading(self.text_after(token))
    }

    /// Returns the whitespace and whitespace byte-length *before* the `token`
    pub(crate) fn leading_whitespace(&self, token: &'a LogicalLineToken) -> (Whitespace, TextSize) {
        Whitespace::trailing(self.text_before(token))
    }

    /// Returns all tokens of the line, including comments and trailing new lines.
    pub(crate) fn tokens(&self) -> &'a [LogicalLineToken] {
        &self.lines.tokens[self.line.tokens_start as usize..self.line.tokens_end as usize]
    }

    pub(crate) fn first_token(&self) -> Option<&'a LogicalLineToken> {
        self.tokens().first()
    }

    /// Returns the line's flags
    pub(crate) const fn flags(&self) -> TokenFlags {
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
pub(crate) struct LogicalLinesIter<'a> {
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

/// A token of a [`LogicalLine`]
#[derive(Clone, Debug)]
pub(crate) struct LogicalLineToken {
    kind: TokenKind,
    range: TextRange,
}

impl LogicalLineToken {
    /// Returns the token's kind
    #[inline]
    pub(crate) const fn kind(&self) -> TokenKind {
        self.kind
    }
}

impl Ranged for LogicalLineToken {
    /// Returns a tuple with the token's `(start, end)` locations
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum Whitespace {
    None,
    Single,
    Many,
    Tab,
}

impl Whitespace {
    fn leading(content: &str) -> (Self, TextSize) {
        let mut count = 0u32;
        let mut len = TextSize::default();
        let mut has_tabs = false;

        for c in content.chars() {
            if c == '#' {
                // Ignore leading whitespace between a token and an end-of-line comment
                return (Whitespace::None, TextSize::default());
            } else if c == '\t' {
                has_tabs = true;
                len += c.text_len();
            } else if matches!(c, '\n' | '\r') {
                break;
            } else if is_python_whitespace(c) {
                count += 1;
                len += c.text_len();
            } else {
                break;
            }
        }

        if has_tabs {
            (Whitespace::Tab, len)
        } else {
            match count {
                0 => (Whitespace::None, len),
                1 => (Whitespace::Single, len),
                _ => (Whitespace::Many, len),
            }
        }
    }

    fn trailing(content: &str) -> (Self, TextSize) {
        let mut len = TextSize::default();
        let mut count = 0usize;
        let mut has_tabs = false;

        for c in content.chars().rev() {
            if c == '\t' {
                has_tabs = true;
                len += c.text_len();
            } else if matches!(c, '\n' | '\r') {
                // Indent
                return (Self::None, TextSize::default());
            } else if is_python_whitespace(c) {
                count += 1;
                len += c.text_len();
            } else {
                break;
            }
        }

        if has_tabs {
            (Self::Tab, len)
        } else {
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
}

#[derive(Debug, Default)]
struct CurrentLine {
    flags: TokenFlags,
    tokens_start: u32,
}

/// Builder for [`LogicalLines`]
#[derive(Debug, Default)]
struct LogicalLinesBuilder {
    tokens: Vec<LogicalLineToken>,
    lines: Vec<Line>,
    current_line: CurrentLine,
}

impl LogicalLinesBuilder {
    fn with_capacity(tokens: usize) -> Self {
        Self {
            tokens: Vec::with_capacity(tokens),
            ..Self::default()
        }
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    #[allow(clippy::cast_possible_truncation)]
    fn push_token(&mut self, kind: TokenKind, range: TextRange) {
        let line = &mut self.current_line;

        if matches!(kind, TokenKind::Comment) {
            line.flags.insert(TokenFlags::COMMENT);
        } else if kind.is_operator() {
            line.flags.insert(TokenFlags::OPERATOR);

            line.flags.set(
                TokenFlags::BRACKET,
                matches!(
                    kind,
                    TokenKind::Lpar
                        | TokenKind::Lsqb
                        | TokenKind::Lbrace
                        | TokenKind::Rpar
                        | TokenKind::Rsqb
                        | TokenKind::Rbrace
                ),
            );
        }

        if matches!(kind, TokenKind::Comma | TokenKind::Semi | TokenKind::Colon) {
            line.flags.insert(TokenFlags::PUNCTUATION);
        } else if kind.is_keyword() {
            line.flags.insert(TokenFlags::KEYWORD);
        }

        line.flags.set(
            TokenFlags::NON_TRIVIA,
            !matches!(
                kind,
                TokenKind::Comment
                    | TokenKind::Newline
                    | TokenKind::NonLogicalNewline
                    | TokenKind::Dedent
                    | TokenKind::Indent
            ),
        );

        self.tokens.push(LogicalLineToken { kind, range });
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    #[allow(clippy::cast_possible_truncation)]
    fn finish_line(&mut self) {
        let end = self.tokens.len() as u32;
        if self.current_line.tokens_start < end {
            let is_empty = self.tokens[self.current_line.tokens_start as usize..end as usize]
                .iter()
                .all(|token| token.kind.is_newline());
            if !is_empty {
                self.lines.push(Line {
                    flags: self.current_line.flags,
                    tokens_start: self.current_line.tokens_start,
                    tokens_end: end,
                });
            }

            self.current_line = CurrentLine {
                flags: TokenFlags::default(),
                tokens_start: end,
            }
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

#[cfg(test)]
mod tests {
    use ruff_python_parser::lexer::LexResult;
    use ruff_python_parser::{lexer, Mode};

    use ruff_source_file::Locator;

    use super::LogicalLines;

    #[test]
    fn multi_line() {
        assert_logical_lines(
            r#"
x = 1
y = 2
z = x + 1"#
                .trim(),
            &["x = 1", "y = 2", "z = x + 1"],
        );
    }

    #[test]
    fn indented() {
        assert_logical_lines(
            r#"
x = [
  1,
  2,
  3,
]
y = 2
z = x + 1"#
                .trim(),
            &["x = [\n  1,\n  2,\n  3,\n]", "y = 2", "z = x + 1"],
        );
    }

    #[test]
    fn string_assignment() {
        assert_logical_lines("x = 'abc'".trim(), &["x = 'abc'"]);
    }

    #[test]
    fn function_definition() {
        assert_logical_lines(
            r#"
def f():
  x = 1
f()"#
                .trim(),
            &["def f():", "x = 1", "f()"],
        );
    }

    #[test]
    fn trivia() {
        assert_logical_lines(
            r#"
def f():
  """Docstring goes here."""
  # Comment goes here.
  x = 1
f()"#
                .trim(),
            &[
                "def f():",
                "\"\"\"Docstring goes here.\"\"\"",
                "",
                "x = 1",
                "f()",
            ],
        );
    }

    #[test]
    fn empty_line() {
        assert_logical_lines(
            r#"
if False:

    print()
"#
            .trim(),
            &["if False:", "print()", ""],
        );
    }

    fn assert_logical_lines(contents: &str, expected: &[&str]) {
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(&lxr, &locator)
            .into_iter()
            .map(|line| line.text_trimmed())
            .map(ToString::to_string)
            .collect();
        let expected: Vec<String> = expected.iter().map(ToString::to_string).collect();
        assert_eq!(actual, expected);
    }
}
