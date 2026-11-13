use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;

use bitflags::bitflags;

pub(crate) use extraneous_whitespace::*;
pub(crate) use indentation::*;
pub(crate) use missing_whitespace::*;
pub(crate) use missing_whitespace_after_keyword::*;
pub(crate) use missing_whitespace_around_operator::*;
pub(crate) use redundant_backslash::*;
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_python_trivia::is_python_whitespace;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
pub(crate) use space_around_operator::*;
pub(crate) use whitespace_around_keywords::*;
pub(crate) use whitespace_around_named_parameter_equals::*;
pub(crate) use whitespace_before_comment::*;
pub(crate) use whitespace_before_parameters::*;

use crate::Locator;
use crate::rules::pycodestyle::helpers::is_non_logical_token;

mod extraneous_whitespace;
mod indentation;
mod missing_whitespace;
mod missing_whitespace_after_keyword;
mod missing_whitespace_around_operator;
mod redundant_backslash;
mod space_around_operator;
mod whitespace_around_keywords;
mod whitespace_around_named_parameter_equals;
mod whitespace_before_comment;
mod whitespace_before_parameters;

bitflags! {
    #[derive(Default, Eq, PartialEq, Clone, Copy, Debug)]
    pub(crate) struct TokenFlags: u8 {
        /// Whether the logical line contains an operator.
        const OPERATOR = 1 << 0;
        /// Whether the logical line contains a bracket.
        const BRACKET = 1 << 1;
        /// Whether the logical line contains a punctuation mark.
        const PUNCTUATION = 1 << 2;
        /// Whether the logical line contains a keyword.
        const KEYWORD = 1 << 3;
        /// Whether the logical line contains a comment.
        const COMMENT = 1 << 4;
        /// Whether the logical line contains any non trivia token (no comment, newline, or in/dedent)
        const NON_TRIVIA = 1 << 5;
    }
}

#[derive(Clone)]
pub(crate) struct LogicalLines<'a> {
    tokens: &'a [Token],
    lines: Vec<Line>,
    locator: &'a Locator<'a>,
}

impl<'a> LogicalLines<'a> {
    fn get_line(&'a self, line: &'a Line) -> Option<LogicalLine<'a>> {
        let tokens = &self.tokens[line.tokens_start as usize..line.tokens_end as usize];
        Some(LogicalLine {
            tokens,
            source: self.locator.contents(),
            range: TextRange::new(tokens.first()?.start(), tokens.last()?.end()),
            flags: line.flags,
            is_start_of_file: line.tokens_start == 0,
        })
    }

    pub(crate) fn from_tokens(tokens: &'a Tokens, locator: &'a Locator<'a>) -> Self {
        assert!(u32::try_from(tokens.len()).is_ok());

        let mut builder = LogicalLinesBuilder {
            tokens,
            ..LogicalLinesBuilder::default()
        };
        let mut tokens_iter = tokens.iter_with_context();

        while let Some(token) = tokens_iter.next() {
            builder.push_token(token.kind());

            if token.kind().is_any_newline() && !tokens_iter.in_parenthesized_context() {
                builder.finish_line();
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
/// This expression forms one logical line because the array elements are parenthesized.
///
/// ```python
/// a = [
///     1,
///     2
/// ]
/// ```
#[derive(Debug)]
pub(crate) struct LogicalLine<'a> {
    tokens: &'a [Token],
    source: &'a str,
    range: TextRange,
    flags: TokenFlags,
    is_start_of_file: bool,
}

impl<'a> LogicalLine<'a> {
    /// Returns `true` if this line is positioned at the start of the file.
    const fn is_start_of_file(&self) -> bool {
        self.is_start_of_file
    }

    /// Returns `true` if this is a comment only line
    pub(crate) fn is_comment_only(&self) -> bool {
        self.flags() == TokenFlags::COMMENT
    }

    /// Returns logical line's text including comments, indents, dedent and trailing new lines.
    fn text(&self) -> &'a str {
        &self.source[self.range]
    }

    /// Returns the text without any leading or trailing newline, comment, indent, or dedent of this line
    #[cfg(test)]
    fn text_trimmed(&self) -> &'a str {
        let tokens = self.tokens_trimmed();

        match (tokens.first(), tokens.last()) {
            (Some(first), Some(last)) => &self.source[TextRange::new(first.start(), last.end())],
            _ => "",
        }
    }

    fn tokens_trimmed(&self) -> &'a [Token] {
        let tokens = self.tokens();

        let start = tokens
            .iter()
            .position(|t| !is_non_logical_token(t.kind()))
            .unwrap_or(tokens.len());

        let tokens = &tokens[start..];

        let end = tokens
            .iter()
            .rposition(|t| !is_non_logical_token(t.kind()))
            .map_or(0, |pos| pos + 1);

        &tokens[..end]
    }

    /// Returns the text after `token` up to the end of this logical line.
    #[inline]
    fn text_after(&self, token: &'a Token) -> &str {
        &self.source[TextRange::new(token.end(), self.range.end())]
    }

    #[inline]
    fn trailing_whitespace(&self, token: &'a Token) -> (Whitespace, TextSize) {
        let after = self.text_after(token);
        match after.as_bytes() {
            [b' ', next, ..] if !matches!(next, b' ' | b'\t' | 0x0c | b'#') => {
                (Whitespace::Single, TextSize::new(1))
            }
            [b' ' | b'\t' | 0x0c, ..] => Whitespace::leading(after),
            _ => (Whitespace::None, TextSize::default()),
        }
    }

    #[inline]
    fn leading_whitespace(&self, token: &'a Token) -> (Whitespace, TextSize) {
        let before = &self.source[TextRange::new(self.range.start(), token.start())];
        match before.as_bytes() {
            [.., prev, b' '] if !matches!(prev, b' ' | b'\t' | 0x0c | b'\r' | b'\n') => {
                (Whitespace::Single, TextSize::new(1))
            }
            [.., b' ' | b'\t' | 0x0c] => Whitespace::trailing(before),
            _ => (Whitespace::None, TextSize::default()),
        }
    }

    /// Returns all tokens of the line, including comments and trailing new lines.
    fn tokens(&self) -> &'a [Token] {
        self.tokens
    }

    pub(crate) fn first_token(&self) -> Option<&'a Token> {
        self.tokens().first()
    }

    /// Returns the line's flags
    pub(crate) const fn flags(&self) -> TokenFlags {
        self.flags
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
        self.lines.get_line(self.inner.next()?)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for LogicalLinesIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.lines.get_line(self.inner.next_back()?)
    }
}

impl ExactSizeIterator for LogicalLinesIter<'_> {}

impl FusedIterator for LogicalLinesIter<'_> {}

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

        if len == content.text_len() {
            // All whitespace up to the start of the line -> Indent
            (Self::None, TextSize::default())
        } else if has_tabs {
            (Self::Tab, len)
        } else {
            match count {
                0 => (Self::None, TextSize::default()),
                1 => (Self::Single, len),
                _ => (Self::Many, len),
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
struct LogicalLinesBuilder<'a> {
    tokens: &'a [Token],
    tokens_end: u32,
    lines: Vec<Line>,
    current_line: CurrentLine,
}

impl<'a> LogicalLinesBuilder<'a> {
    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    fn push_token(&mut self, kind: TokenKind) {
        let line = &mut self.current_line;

        if matches!(kind, TokenKind::Comment) {
            line.flags.insert(TokenFlags::COMMENT);
        } else if kind.is_operator() {
            line.flags.insert(TokenFlags::OPERATOR);

            if matches!(
                kind,
                TokenKind::Lpar
                    | TokenKind::Lsqb
                    | TokenKind::Lbrace
                    | TokenKind::Rpar
                    | TokenKind::Rsqb
                    | TokenKind::Rbrace
            ) {
                line.flags.insert(TokenFlags::BRACKET);
            }
        }

        if matches!(kind, TokenKind::Comma | TokenKind::Semi | TokenKind::Colon) {
            line.flags.insert(TokenFlags::PUNCTUATION);
        } else if kind.is_keyword() {
            line.flags.insert(TokenFlags::KEYWORD);
        }

        if !is_non_logical_token(kind) {
            line.flags.insert(TokenFlags::NON_TRIVIA);
        }

        self.tokens_end += 1;
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    fn finish_line(&mut self) {
        let end = self.tokens_end;
        if self.current_line.tokens_start < end {
            let is_empty = self.tokens[self.current_line.tokens_start as usize..end as usize]
                .iter()
                .all(|token| token.kind().is_any_newline());
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

    fn finish(mut self, locator: &'a Locator<'a>) -> LogicalLines<'a> {
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

/// Keeps track of whether we are currently visiting a class or function definition in a
/// [`LogicalLine`]. If we are visiting a class or function, the enum also keeps track
/// of the [type parameters] of the class/function.
///
/// Call [`DefinitionState::visit_token_kind`] on the [`TokenKind`] of each
/// successive [`Token`] to ensure the state remains up to date.
///
/// [type parameters]: https://docs.python.org/3/reference/compound_stmts.html#type-params
#[derive(Debug, Clone, Copy)]
enum DefinitionState {
    InClass(TypeParamsState),
    InFunction(TypeParamsState),
    InTypeAlias(TypeParamsState),
    NotInDefinition,
}

impl DefinitionState {
    fn from_tokens<'a>(tokens: impl IntoIterator<Item = &'a Token>) -> Self {
        let mut token_kinds = tokens.into_iter().map(Token::kind);
        while let Some(token_kind) = token_kinds.next() {
            let state = match token_kind {
                TokenKind::Indent | TokenKind::Dedent => continue,
                TokenKind::Class => Self::InClass(TypeParamsState::default()),
                TokenKind::Def => Self::InFunction(TypeParamsState::default()),
                TokenKind::Async if matches!(token_kinds.next(), Some(TokenKind::Def)) => {
                    Self::InFunction(TypeParamsState::default())
                }
                TokenKind::Type => Self::InTypeAlias(TypeParamsState::default()),
                _ => Self::NotInDefinition,
            };
            return state;
        }
        Self::NotInDefinition
    }

    const fn in_function_definition(self) -> bool {
        matches!(self, Self::InFunction(_))
    }

    const fn type_params_state(self) -> Option<TypeParamsState> {
        match self {
            Self::InClass(state) | Self::InFunction(state) | Self::InTypeAlias(state) => {
                Some(state)
            }
            Self::NotInDefinition => None,
        }
    }

    fn in_type_params(self) -> bool {
        matches!(
            self.type_params_state(),
            Some(TypeParamsState::InTypeParams { .. })
        )
    }

    fn visit_token_kind(&mut self, token_kind: TokenKind) {
        let type_params_state_mut = match self {
            Self::InClass(type_params_state)
            | Self::InFunction(type_params_state)
            | Self::InTypeAlias(type_params_state) => type_params_state,
            Self::NotInDefinition => return,
        };
        match token_kind {
            TokenKind::Lpar | TokenKind::Equal if type_params_state_mut.before_type_params() => {
                *type_params_state_mut = TypeParamsState::TypeParamsEnded;
            }
            TokenKind::Lsqb => match type_params_state_mut {
                TypeParamsState::TypeParamsEnded => {}
                TypeParamsState::BeforeTypeParams => {
                    *type_params_state_mut = TypeParamsState::InTypeParams {
                        inner_square_brackets: 0,
                    };
                }
                TypeParamsState::InTypeParams {
                    inner_square_brackets,
                } => *inner_square_brackets += 1,
            },
            TokenKind::Rsqb => {
                if let TypeParamsState::InTypeParams {
                    inner_square_brackets,
                } = type_params_state_mut
                {
                    if *inner_square_brackets == 0 {
                        *type_params_state_mut = TypeParamsState::TypeParamsEnded;
                    } else {
                        *inner_square_brackets -= 1;
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum TypeParamsState {
    #[default]
    BeforeTypeParams,
    InTypeParams {
        inner_square_brackets: u32,
    },
    TypeParamsEnded,
}

impl TypeParamsState {
    const fn before_type_params(self) -> bool {
        matches!(self, Self::BeforeTypeParams)
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::parse_module;

    use crate::Locator;

    use super::LogicalLines;

    #[test]
    fn multi_line() {
        assert_logical_lines(
            r"
x = 1
y = 2
z = x + 1"
                .trim(),
            &["x = 1", "y = 2", "z = x + 1"],
        );
    }

    #[test]
    fn indented() {
        assert_logical_lines(
            r"
x = [
  1,
  2,
  3,
]
y = 2
z = x + 1"
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
            r"
def f():
  x = 1
f()"
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
            r"
if False:

    print()
"
            .trim(),
            &["if False:", "print()", ""],
        );
    }

    fn assert_logical_lines(contents: &str, expected: &[&str]) {
        let parsed = parse_module(contents).unwrap();
        let locator = Locator::new(contents);
        let actual: Vec<String> = LogicalLines::from_tokens(parsed.tokens(), &locator)
            .into_iter()
            .map(|line| line.text_trimmed())
            .map(ToString::to_string)
            .collect();
        let expected: Vec<String> = expected.iter().map(ToString::to_string).collect();
        assert_eq!(actual, expected);
    }
}
