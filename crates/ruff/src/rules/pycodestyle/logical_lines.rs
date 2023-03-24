use bitflags::bitflags;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;
use std::borrow::Cow;
use std::iter::Flatten;
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

#[derive(Debug)]
pub struct LogicalLine<'a> {
    pub text: String,
    pub mapping: Vec<(usize, Location)>,
    pub flags: TokenFlags,
    pub tokens: Vec<(Location, &'a Tok, Location)>,
}

impl<'a> LogicalLine<'a> {
    pub fn is_comment(&self) -> bool {
        self.text.is_empty()
    }
}

struct LineBuilder<'a> {
    tokens: Vec<(Location, &'a Tok, Location)>,
    // BTreeMap?
    mappings: Vec<(usize, Location)>,
    text: String,
    flags: TokenFlags,
    previous: Option<Location>,
}

impl<'a> LineBuilder<'a> {
    fn new() -> Self {
        Self {
            tokens: Vec::with_capacity(32),
            mappings: Vec::new(),
            text: String::with_capacity(88),
            flags: TokenFlags::empty(),
            previous: None,
        }
    }

    fn push_token(&mut self, start: Location, token: &'a Tok, end: Location, locator: &Locator) {
        self.tokens.push((start, token, end));

        if matches!(
            token,
            Tok::Newline | Tok::NonLogicalNewline | Tok::Indent | Tok::Dedent
        ) {
            return;
        }

        if self.mappings.is_empty() {
            self.mappings.push((0, start));
        }

        if matches!(token, Tok::Comment(..)) {
            self.flags.insert(TokenFlags::COMMENT);
            return;
        }

        if is_op_token(token) {
            self.flags.insert(TokenFlags::OPERATOR);
        }

        if matches!(
            token,
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace | Tok::Rpar | Tok::Rsqb | Tok::Rbrace
        ) {
            self.flags.insert(TokenFlags::BRACKET);
        }

        if matches!(token, Tok::Comma | Tok::Semi | Tok::Colon) {
            self.flags.insert(TokenFlags::PUNCTUATION);
        }

        if is_keyword_token(token) {
            self.flags.insert(TokenFlags::KEYWORD);
        }

        // TODO(charlie): "Mute" strings.
        let text = if let Tok::String { value, .. } = token {
            Cow::Owned(format!("\"{}\"", "x".repeat(value.width())))
        } else {
            Cow::Borrowed(locator.slice(Range {
                location: start,
                end_location: end,
            }))
        };

        if let Some(prev) = self.previous {
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

        self.text.push_str(&text);
        self.mappings.push((self.text.len(), end));
        self.previous = Some(end);
    }

    fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    fn finish(self) -> LogicalLine<'a> {
        LogicalLine {
            text: self.text,
            mapping: self.mappings,
            flags: self.flags,
            tokens: self.tokens,
        }
    }
}

pub fn iter_logical_lines<'a>(
    tokens: &'a [LexResult],
    locator: &'a Locator,
) -> LogicalLinesIterator<'a> {
    LogicalLinesIterator::new(tokens, locator)
}

pub struct LogicalLinesIterator<'a> {
    tokens: Flatten<std::slice::Iter<'a, LexResult>>,
    locator: &'a Locator<'a>,
    parens: usize,
    single_item: bool,
}

impl<'a> LogicalLinesIterator<'a> {
    fn new(tokens: &'a [LexResult], locator: &'a Locator<'a>) -> Self {
        Self {
            single_item: tokens.len() == 1,
            locator,
            parens: 0,
            tokens: tokens.iter().flatten(),
        }
    }
}

impl<'a> Iterator for LogicalLinesIterator<'a> {
    type Item = LogicalLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line_builder = LineBuilder::new();

        for (start, token, end) in self.tokens.by_ref() {
            line_builder.push_token(*start, token, *end, &self.locator);

            match token {
                Tok::Lbrace | Tok::Lpar | Tok::Lsqb => {
                    self.parens += 1;
                }
                Tok::Rbrace | Tok::Rpar | Tok::Rsqb => {
                    self.parens -= 1;
                }
                Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(_) if self.parens == 0 => {
                    return if matches!(token, Tok::Newline) {
                        Some(line_builder.finish())
                    }
                    // Comment only file or non logical new line?
                    else if self.single_item {
                        None
                    } else {
                        Some(line_builder.finish())
                    };
                }
                _ => {}
            }
        }

        if !line_builder.is_empty() {
            Some(line_builder.finish())
        } else {
            None
        }
    }
}
