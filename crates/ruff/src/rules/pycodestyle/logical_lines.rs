use bitflags::bitflags;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;
use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;
use unicode_width::UnicodeWidthStr;

use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

use crate::rules::pycodestyle::helpers::{is_keyword_token, is_op_token};

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
    text: String,

    /// start position, token, end position
    tokens: Vec<(Location, &'a Tok, Location)>,

    mappings: Mappings,

    lines: Vec<Line>,
}

impl<'a> LogicalLines<'a> {
    pub fn from_tokens(tokens: &'a [LexResult], locator: &Locator) -> Self {
        assert!(u32::try_from(tokens.len()).is_ok());

        let single_token = tokens.len() == 1;
        let mut builder =
            LogicalLinesBuilder::with_capacity(tokens.len(), locator.contents().len());
        let mut parens: u32 = 0;

        for (start, token, end) in tokens.iter().flatten() {
            builder.push_token(*start, token, *end, locator);

            match token {
                Tok::Lbrace | Tok::Lpar | Tok::Lsqb => {
                    parens += 1;
                }
                Tok::Rbrace | Tok::Rpar | Tok::Rsqb => {
                    parens -= 1;
                }
                Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(_) if parens == 0 => {
                    if matches!(token, Tok::Newline) {
                        builder.finish_line();
                    }
                    // Comment only file or non logical new line?
                    else if single_token {
                        builder.discard_line();
                    } else {
                        builder.finish_line();
                    };
                }
                _ => {}
            }
        }

        builder.finish()
    }
}

impl std::fmt::Debug for LogicalLines<'_> {
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

#[derive(Debug, Clone)]
struct Line {
    flags: TokenFlags,
    /// Byte offset of the start of the text of this line.
    text_start: u32,

    /// Byte offset of the end of the text of this line.
    text_end: u32,
    mappings_start: u32,
    mappings_end: u32,
    tokens_start: u32,
    tokens_end: u32,
}

#[derive(Debug)]
pub struct LogicalLine<'a> {
    lines: &'a LogicalLines<'a>,
    line: &'a Line,
}

impl<'a> LogicalLine<'a> {
    /// Returns true if this is a comment only line
    pub fn is_comment(&self) -> bool {
        self.text().is_empty() && self.flags().contains(TokenFlags::COMMENT)
    }

    /// Returns the text of this line
    pub fn text(&self) -> &'a str {
        &self.lines.text[self.line.text_start as usize..self.line.text_end as usize]
    }

    /// Returns the tokens of the line
    pub fn tokens(&self) -> &'a [(Location, &'a Tok, Location)] {
        &self.lines.tokens[self.line.tokens_start as usize..self.line.tokens_end as usize]
    }

    /// Returns the [`Location`] of the first token on the line or [`None`].
    pub fn first_token_location(&self) -> Option<&Location> {
        self.token_locations().first()
    }

    fn token_offsets(&self) -> &[u32] {
        &self.lines.mappings.logical_line_offsets
            [self.line.mappings_start as usize..self.line.mappings_end as usize]
    }

    fn token_locations(&self) -> &[Location] {
        &self.lines.mappings.locations
            [self.line.mappings_start as usize..self.line.mappings_end as usize]
    }

    /// Returns the mapping for an offset in the logical line.
    ///
    /// The offset of the closest token and its corresponding location.
    pub fn mapping(&self, offset: usize) -> (usize, Location) {
        let index = self
            .token_offsets()
            .binary_search(&(self.line.text_start + u32::try_from(offset).unwrap()))
            .unwrap_or_default();

        (
            (self.token_offsets()[index] - self.line.text_start) as usize,
            self.token_locations()[index],
        )
    }

    pub fn is_empty(&self) -> bool {
        self.lines.mappings.is_empty()
    }

    pub const fn flags(&self) -> TokenFlags {
        self.line.flags
    }
}

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

/// Source map that maps byte positions in the logical line text to the [`Location`] in the
/// original document.
#[derive(Debug, Default, Clone)]
struct Mappings {
    /// byte offsets of the logical lines at which tokens start/end.
    logical_line_offsets: Vec<u32>,

    /// Corresponding [`Location`]s for each byte offset mapping it to the position in the original document.
    locations: Vec<Location>,
}

impl Mappings {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            logical_line_offsets: Vec::with_capacity(capacity),
            locations: Vec::with_capacity(capacity),
        }
    }

    fn len(&self) -> usize {
        self.logical_line_offsets.len()
    }

    fn is_empty(&self) -> bool {
        self.logical_line_offsets.is_empty()
    }

    fn truncate(&mut self, len: usize) {
        self.locations.truncate(len);
        self.logical_line_offsets.truncate(len);
    }

    #[allow(clippy::cast_possible_truncation)]
    fn push(&mut self, offset: usize, location: Location) {
        self.logical_line_offsets.push(offset as u32);
        self.locations.push(location);
    }
}

#[derive(Debug, Default)]
struct CurrentLine {
    flags: TokenFlags,
    text_start: u32,
    mappings_start: u32,
    tokens_start: u32,
    previous_token: Option<Location>,
}

#[derive(Debug, Default)]
pub struct LogicalLinesBuilder<'a> {
    text: String,
    tokens: Vec<(Location, &'a Tok, Location)>,
    mappings: Mappings,
    lines: Vec<Line>,
    current_line: Option<CurrentLine>,
}

impl<'a> LogicalLinesBuilder<'a> {
    fn with_capacity(tokens: usize, string: usize) -> Self {
        Self {
            tokens: Vec::with_capacity(tokens),
            mappings: Mappings::with_capacity(tokens + 1),
            text: String::with_capacity(string),
            ..Self::default()
        }
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    #[allow(clippy::cast_possible_truncation)]
    fn push_token(&mut self, start: Location, token: &'a Tok, end: Location, locator: &Locator) {
        let tokens_start = self.tokens.len();
        self.tokens.push((start, token, end));

        let mut line = self.current_line.get_or_insert_with(|| {
            let mappings_start = self.mappings.len();
            self.mappings.push(self.text.len(), start);

            CurrentLine {
                flags: TokenFlags::empty(),
                text_start: self.text.len() as u32,
                mappings_start: mappings_start as u32,
                tokens_start: tokens_start as u32,
                previous_token: None,
            }
        });

        if matches!(
            token,
            Tok::Newline | Tok::NonLogicalNewline | Tok::Indent | Tok::Dedent
        ) {
            return;
        }

        if matches!(token, Tok::Comment(..)) {
            line.flags.insert(TokenFlags::COMMENT);
            return;
        }

        if is_op_token(token) {
            line.flags.insert(TokenFlags::OPERATOR);
        }

        if matches!(
            token,
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace | Tok::Rpar | Tok::Rsqb | Tok::Rbrace
        ) {
            line.flags.insert(TokenFlags::BRACKET);
        }

        if matches!(token, Tok::Comma | Tok::Semi | Tok::Colon) {
            line.flags.insert(TokenFlags::PUNCTUATION);
        }

        if is_keyword_token(token) {
            line.flags.insert(TokenFlags::KEYWORD);
        }

        // TODO(charlie): "Mute" strings.
        let text = if let Tok::String { value, .. } = token {
            // Replace the content of strings with a non-whs sequence because some lints
            // search for whitespace in the document and whitespace inside of the strinig
            // would complicate the search.
            Cow::Owned(format!("\"{}\"", "x".repeat(value.width())))
        } else {
            Cow::Borrowed(locator.slice(Range {
                location: start,
                end_location: end,
            }))
        };

        if let Some(prev) = line.previous_token.take() {
            if prev.row() != start.row() {
                let prev_text = locator.slice(Range {
                    location: Location::new(prev.row(), prev.column() - 1),
                    end_location: Location::new(prev.row(), prev.column()),
                });
                if prev_text == ","
                    || ((prev_text != "{" && prev_text != "[" && prev_text != "(")
                        && (text != "}" && text != "]" && text != ")"))
                {
                    self.text.push(' ');
                }
            } else if prev.column() != start.column() {
                let prev_text = locator.slice(Range {
                    location: prev,
                    end_location: start,
                });
                self.text.push_str(prev_text);
            }
        }

        line.previous_token = Some(end);
        self.text.push_str(&text);
        self.mappings.push(self.text.len(), end);
    }

    // SAFETY: `LogicalLines::from_tokens` asserts that the file has less than `u32::MAX` tokens and each tokens is at least one character long
    #[allow(clippy::cast_possible_truncation)]
    fn finish_line(&mut self) {
        if let Some(current) = self.current_line.take() {
            self.lines.push(Line {
                flags: current.flags,
                text_start: current.text_start,
                text_end: self.text.len() as u32,
                mappings_start: current.mappings_start,
                mappings_end: self.mappings.len() as u32,
                tokens_start: current.tokens_start,
                tokens_end: self.tokens.len() as u32,
            });
        }
    }

    fn discard_line(&mut self) {
        if let Some(current) = self.current_line.take() {
            self.text.truncate(current.text_start as usize);
            self.tokens.truncate(current.tokens_start as usize);
            self.mappings.truncate(current.mappings_start as usize);
        }
    }

    fn finish(mut self) -> LogicalLines<'a> {
        self.finish_line();

        LogicalLines {
            text: self.text,
            tokens: self.tokens,
            mappings: self.mappings,
            lines: self.lines,
        }
    }
}
