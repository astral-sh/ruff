use rustpython_ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::source_code::Locator;

#[derive(Debug)]
pub struct LogicalLine {
    pub text: String,
    pub mapping: Vec<(usize, Location)>,
    /// Whether the logical line contains an operator.
    pub operator: bool,
    /// Whether the logical line contains a comment.
    pub bracket: bool,
    /// Whether the logical line contains a punctuation mark.
    pub punctuation: bool,
}

impl LogicalLine {
    pub fn is_comment(&self) -> bool {
        self.text.is_empty()
    }
}

fn build_line(tokens: &[(Location, &Tok, Location)], locator: &Locator) -> LogicalLine {
    let mut logical = String::with_capacity(88);
    let mut operator = false;
    let mut bracket = false;
    let mut punctuation = false;
    let mut mapping = Vec::new();
    let mut prev: Option<&Location> = None;
    let mut length = 0;
    for (start, tok, end) in tokens {
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
            continue;
        }

        if !operator {
            operator |= matches!(
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
            );
        }

        if !bracket {
            bracket |= matches!(
                tok,
                Tok::Lpar | Tok::Lsqb | Tok::Lbrace | Tok::Rpar | Tok::Rsqb | Tok::Rbrace
            );
        }

        if !punctuation {
            punctuation |= matches!(tok, Tok::Comma | Tok::Semi | Tok::Colon);
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
        operator,
        bracket,
        punctuation,
        mapping,
    }
}

pub fn iter_logical_lines(tokens: &[LexResult], locator: &Locator) -> Vec<LogicalLine> {
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
                    lines.push(build_line(&accumulator, locator));
                    accumulator.drain(..);
                } else if tokens.len() == 1 {
                    accumulator.remove(0);
                } else {
                    lines.push(build_line(&accumulator, locator));
                    accumulator.drain(..);
                }
            }
        }
    }
    if !accumulator.is_empty() {
        lines.push(build_line(&accumulator, locator));
    }
    lines
}
