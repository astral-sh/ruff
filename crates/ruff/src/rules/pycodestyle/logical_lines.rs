use bitflags::bitflags;
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::source_code::Locator;

bitflags! {
    #[derive(Default)]
    pub struct TokenFlags: u32 {
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

fn build_line<'a>(
    tokens: Vec<(Location, &'a Tok, Location)>,
    locator: &Locator,
) -> LogicalLine<'a> {
    let mut logical = String::with_capacity(88);
    let mut mapping = Vec::new();
    let mut flags = TokenFlags::empty();
    let mut prev: Option<&Location> = None;
    let mut length = 0;
    for (start, tok, end) in &tokens {
        if matches!(
            tok,
            Tok::Newline | Tok::NonLogicalNewline | Tok::Indent | Tok::Dedent
        ) {
            continue;
        }

        if mapping.is_empty() {
            mapping.push((0, *start));
        }

        if matches!(tok, Tok::Comment { .. }) {
            flags.insert(TokenFlags::COMMENT);
            continue;
        }

        if matches!(
            tok,
            Tok::Amper
                | Tok::AmperEqual
                | Tok::CircumFlex
                | Tok::CircumflexEqual
                | Tok::Colon
                | Tok::ColonEqual
                | Tok::DoubleSlash
                | Tok::DoubleSlashEqual
                | Tok::DoubleStar
                | Tok::Equal
                | Tok::Greater
                | Tok::GreaterEqual
                | Tok::Less
                | Tok::LessEqual
                | Tok::Minus
                | Tok::MinusEqual
                | Tok::NotEqual
                | Tok::Percent
                | Tok::PercentEqual
                | Tok::Plus
                | Tok::PlusEqual
                | Tok::Slash
                | Tok::SlashEqual
                | Tok::Star
                | Tok::StarEqual
                | Tok::Vbar
                | Tok::VbarEqual
        ) {
            flags.insert(TokenFlags::OPERATOR);
        }

        if matches!(
            tok,
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace | Tok::Rpar | Tok::Rsqb | Tok::Rbrace
        ) {
            flags.insert(TokenFlags::BRACKET);
        }

        if matches!(tok, Tok::Comma | Tok::Semi | Tok::Colon) {
            flags.insert(TokenFlags::PUNCTUATION);
        }

        if matches!(
            tok,
            Tok::False
                | Tok::None
                | Tok::True
                | Tok::And
                | Tok::As
                | Tok::Assert
                | Tok::Async
                | Tok::Await
                | Tok::Break
                | Tok::Class
                | Tok::Continue
                | Tok::Def
                | Tok::Del
                | Tok::Elif
                | Tok::Else
                | Tok::Except
                | Tok::Finally
                | Tok::For
                | Tok::From
                | Tok::Global
                | Tok::If
                | Tok::Import
                | Tok::In
                | Tok::Is
                | Tok::Lambda
                | Tok::Nonlocal
                | Tok::Not
                | Tok::Or
                | Tok::Pass
                | Tok::Raise
                | Tok::Return
                | Tok::Try
                | Tok::While
                | Tok::With
                | Tok::Yield
        ) {
            flags.insert(TokenFlags::KEYWORD);
        }

        // TODO(charlie): "Mute" strings.
        let text = if let Tok::String { .. } = tok {
            "\"xxx\""
        } else {
            locator.slice_source_code_range(&Range {
                location: *start,
                end_location: *end,
            })
        };

        if let Some(prev) = prev {
            if prev.row() != start.row() {
                let prev_text = locator.slice_source_code_range(&Range {
                    location: Location::new(prev.row(), prev.column() - 1),
                    end_location: Location::new(prev.row(), prev.column()),
                });
                if prev_text == ","
                    || ((prev_text != "{" && prev_text != "[" && prev_text != "(")
                        && (text != "}" && text != "]" && text != ")"))
                {
                    logical.push(' ');
                    length += 1;
                }
            } else if prev.column() != start.column() {
                let prev_text = locator.slice_source_code_range(&Range {
                    location: *prev,
                    end_location: *start,
                });
                logical.push_str(prev_text);
                length += prev_text.len();
            }
        }
        logical.push_str(text);
        length += text.len();
        mapping.push((length, *end));
        prev = Some(end);
    }

    LogicalLine {
        text: logical,
        mapping,
        flags,
        tokens,
    }
}

pub fn iter_logical_lines<'a>(tokens: &'a [LexResult], locator: &Locator) -> Vec<LogicalLine<'a>> {
    let mut parens = 0;
    let mut accumulator = Vec::with_capacity(32);
    let mut lines = Vec::with_capacity(128);
    for &(start, ref tok, end) in tokens.iter().flatten() {
        accumulator.push((start, tok, end));
        if matches!(tok, Tok::Lbrace | Tok::Lpar | Tok::Lsqb) {
            parens += 1;
        } else if matches!(tok, Tok::Rbrace | Tok::Rpar | Tok::Rsqb) {
            parens -= 1;
        } else if parens == 0 {
            if matches!(
                tok,
                Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(..)
            ) {
                if matches!(tok, Tok::Newline) {
                    lines.push(build_line(accumulator, locator));
                    accumulator = Vec::with_capacity(32);
                } else if tokens.len() == 1 {
                    accumulator.remove(0);
                } else {
                    lines.push(build_line(accumulator, locator));
                    accumulator = Vec::with_capacity(32);
                }
            }
        }
    }
    if !accumulator.is_empty() {
        lines.push(build_line(accumulator, locator));
    }
    lines
}
