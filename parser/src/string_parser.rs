use self::FStringErrorType::*;
use crate::{
    ast::{Constant, ConversionFlag, Expr, ExprKind, Location},
    error::{FStringErrorType, LexicalError, LexicalErrorType, ParseError},
    parser::parse_expression_located,
    token::StringKind,
};
use std::{iter, mem, str};

/// unicode_name2 does not expose `MAX_NAME_LENGTH`, so we replicate that constant here, fix #3798
pub const MAX_UNICODE_NAME: usize = 88;

pub struct StringParser<'a> {
    chars: iter::Peekable<str::Chars<'a>>,
    kind: StringKind,
    str_start: Location,
    str_end: Location,
    location: Location,
}

impl<'a> StringParser<'a> {
    pub fn new(
        source: &'a str,
        kind: StringKind,
        triple_quoted: bool,
        str_start: Location,
        str_end: Location,
    ) -> Self {
        let offset = kind.to_string().len() + if triple_quoted { 3 } else { 1 };
        Self {
            chars: source.chars().peekable(),
            kind,
            str_start,
            str_end,
            location: Location::new(str_start.row(), str_start.column() + offset),
        }
    }

    #[inline]
    fn next_char(&mut self) -> Option<char> {
        let Some(c) = self.chars.next() else {
            return None
        };
        if c == '\n' {
            self.location.newline();
        } else {
            self.location.go_right();
        }
        Some(c)
    }

    #[inline]
    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    #[inline]
    fn get_pos(&self) -> Location {
        self.location
    }

    #[inline]
    fn expr(&self, node: ExprKind) -> Expr {
        Expr::new(self.str_start, self.str_end, node)
    }

    fn parse_unicode_literal(&mut self, literal_number: usize) -> Result<char, LexicalError> {
        let mut p: u32 = 0u32;
        let unicode_error = LexicalError {
            error: LexicalErrorType::UnicodeError,
            location: self.get_pos(),
        };
        for i in 1..=literal_number {
            match self.next_char() {
                Some(c) => match c.to_digit(16) {
                    Some(d) => p += d << ((literal_number - i) * 4),
                    None => return Err(unicode_error),
                },
                None => return Err(unicode_error),
            }
        }
        match p {
            0xD800..=0xDFFF => Ok(std::char::REPLACEMENT_CHARACTER),
            _ => std::char::from_u32(p).ok_or(unicode_error),
        }
    }

    fn parse_octet(&mut self, first: char) -> char {
        let mut octet_content = String::new();
        octet_content.push(first);
        while octet_content.len() < 3 {
            if let Some('0'..='7') = self.peek() {
                octet_content.push(self.next_char().unwrap())
            } else {
                break;
            }
        }
        let value = u32::from_str_radix(&octet_content, 8).unwrap();
        char::from_u32(value).unwrap()
    }

    fn parse_unicode_name(&mut self) -> Result<char, LexicalError> {
        let start_pos = self.get_pos();
        match self.next_char() {
            Some('{') => {}
            _ => {
                return Err(LexicalError {
                    error: LexicalErrorType::StringError,
                    location: start_pos,
                })
            }
        }
        let start_pos = self.get_pos();
        let mut name = String::new();
        loop {
            match self.next_char() {
                Some('}') => break,
                Some(c) => name.push(c),
                None => {
                    return Err(LexicalError {
                        error: LexicalErrorType::StringError,
                        location: self.get_pos(),
                    })
                }
            }
        }

        if name.len() > MAX_UNICODE_NAME {
            return Err(LexicalError {
                error: LexicalErrorType::UnicodeError,
                location: self.get_pos(),
            });
        }

        unicode_names2::character(&name).ok_or(LexicalError {
            error: LexicalErrorType::UnicodeError,
            location: start_pos,
        })
    }

    fn parse_escaped_char(&mut self) -> Result<String, LexicalError> {
        match self.next_char() {
            Some(c) => Ok(match c {
                '\\' => '\\'.to_string(),
                '\'' => '\''.to_string(),
                '\"' => '"'.to_string(),
                '\n' => "".to_string(),
                'a' => '\x07'.to_string(),
                'b' => '\x08'.to_string(),
                'f' => '\x0c'.to_string(),
                'n' => '\n'.to_string(),
                'r' => '\r'.to_string(),
                't' => '\t'.to_string(),
                'v' => '\x0b'.to_string(),
                o @ '0'..='7' => self.parse_octet(o).to_string(),
                'x' => self.parse_unicode_literal(2)?.to_string(),
                'u' if !self.kind.is_bytes() => self.parse_unicode_literal(4)?.to_string(),
                'U' if !self.kind.is_bytes() => self.parse_unicode_literal(8)?.to_string(),
                'N' if !self.kind.is_bytes() => self.parse_unicode_name()?.to_string(),
                c => {
                    if self.kind.is_bytes() && !c.is_ascii() {
                        return Err(LexicalError {
                            error: LexicalErrorType::OtherError(
                                "bytes can only contain ASCII literal characters".to_owned(),
                            ),
                            location: self.get_pos(),
                        });
                    }
                    format!("\\{c}")
                }
            }),
            None => Err(LexicalError {
                error: LexicalErrorType::StringError,
                location: self.get_pos(),
            }),
        }
    }

    fn parse_formatted_value(&mut self, nested: u8) -> Result<Vec<Expr>, LexicalError> {
        let mut expression = String::new();
        let mut spec = None;
        let mut delims = Vec::new();
        let mut conversion = ConversionFlag::None;
        let mut self_documenting = false;
        let mut trailing_seq = String::new();
        let location = self.get_pos();

        while let Some(ch) = self.next_char() {
            match ch {
                // can be integrated better with the remaining code, but as a starting point ok
                // in general I would do here a tokenizing of the fstrings to omit this peeking.
                '!' | '=' | '>' | '<' if self.peek() == Some(&'=') => {
                    expression.push(ch);
                    expression.push('=');
                    self.next_char();
                }
                '!' if delims.is_empty() && self.peek() != Some(&'=') => {
                    if expression.trim().is_empty() {
                        return Err(EmptyExpression.to_lexical_error(self.get_pos()));
                    }

                    conversion = match self.next_char() {
                        Some('s') => ConversionFlag::Str,
                        Some('a') => ConversionFlag::Ascii,
                        Some('r') => ConversionFlag::Repr,
                        Some(_) => {
                            return Err(InvalidConversionFlag.to_lexical_error(self.get_pos()));
                        }
                        None => {
                            return Err(UnclosedLbrace.to_lexical_error(self.get_pos()));
                        }
                    };

                    match self.peek() {
                        Some('}' | ':') => {}
                        Some(_) | None => {
                            return Err(UnclosedLbrace.to_lexical_error(self.get_pos()))
                        }
                    }
                }

                // match a python 3.8 self documenting expression
                // format '{' PYTHON_EXPRESSION '=' FORMAT_SPECIFIER? '}'
                '=' if self.peek() != Some(&'=') && delims.is_empty() => {
                    self_documenting = true;
                }

                ':' if delims.is_empty() => {
                    let parsed_spec = self.parse_spec(nested)?;

                    spec = Some(Box::new(self.expr(ExprKind::JoinedStr {
                        values: parsed_spec,
                    })));
                }
                '(' | '{' | '[' => {
                    expression.push(ch);
                    delims.push(ch);
                }
                ')' => {
                    let last_delim = delims.pop();
                    match last_delim {
                        Some('(') => {
                            expression.push(ch);
                        }
                        Some(c) => {
                            return Err(
                                MismatchedDelimiter(c, ')').to_lexical_error(self.get_pos())
                            );
                        }
                        None => {
                            return Err(Unmatched(')').to_lexical_error(self.get_pos()));
                        }
                    }
                }
                ']' => {
                    let last_delim = delims.pop();
                    match last_delim {
                        Some('[') => {
                            expression.push(ch);
                        }
                        Some(c) => {
                            return Err(
                                MismatchedDelimiter(c, ']').to_lexical_error(self.get_pos())
                            );
                        }
                        None => {
                            return Err(Unmatched(']').to_lexical_error(self.get_pos()));
                        }
                    }
                }
                '}' if !delims.is_empty() => {
                    let last_delim = delims.pop();
                    match last_delim {
                        Some('{') => {
                            expression.push(ch);
                        }
                        Some(c) => {
                            return Err(MismatchedDelimiter(c, '}').to_lexical_error(self.get_pos()))
                        }
                        None => {}
                    }
                }
                '}' => {
                    if expression.trim().is_empty() {
                        return Err(EmptyExpression.to_lexical_error(self.get_pos()));
                    }

                    let ret = if !self_documenting {
                        vec![self.expr(ExprKind::FormattedValue {
                            value: Box::new(parse_fstring_expr(&expression, location).map_err(
                                |e| {
                                    InvalidExpression(Box::new(e.error))
                                        .to_lexical_error(self.get_pos())
                                },
                            )?),
                            conversion: conversion as _,
                            format_spec: spec,
                        })]
                    } else {
                        vec![
                            self.expr(ExprKind::Constant {
                                value: Constant::Str(expression.to_owned() + "="),
                                kind: None,
                            }),
                            self.expr(ExprKind::Constant {
                                value: trailing_seq.into(),
                                kind: None,
                            }),
                            self.expr(ExprKind::FormattedValue {
                                value: Box::new(
                                    parse_fstring_expr(&expression, location).map_err(|e| {
                                        InvalidExpression(Box::new(e.error))
                                            .to_lexical_error(self.get_pos())
                                    })?,
                                ),
                                conversion: (if conversion == ConversionFlag::None && spec.is_none()
                                {
                                    ConversionFlag::Repr
                                } else {
                                    conversion
                                }) as _,
                                format_spec: spec,
                            }),
                        ]
                    };
                    return Ok(ret);
                }
                '"' | '\'' => {
                    expression.push(ch);
                    loop {
                        let Some(c) = self.next_char() else {
                            return Err(UnterminatedString.to_lexical_error(self.get_pos()));
                        };
                        expression.push(c);
                        if c == ch {
                            break;
                        }
                    }
                }
                ' ' if self_documenting => {
                    trailing_seq.push(ch);
                }
                '\\' => return Err(ExpressionCannotInclude('\\').to_lexical_error(self.get_pos())),
                _ => {
                    if self_documenting {
                        return Err(UnclosedLbrace.to_lexical_error(self.get_pos()));
                    }

                    expression.push(ch);
                }
            }
        }
        Err(if expression.trim().is_empty() {
            EmptyExpression.to_lexical_error(self.get_pos())
        } else {
            UnclosedLbrace.to_lexical_error(self.get_pos())
        })
    }

    fn parse_spec(&mut self, nested: u8) -> Result<Vec<Expr>, LexicalError> {
        let mut spec_constructor = Vec::new();
        let mut constant_piece = String::new();
        while let Some(&next) = self.peek() {
            match next {
                '{' => {
                    if !constant_piece.is_empty() {
                        spec_constructor.push(self.expr(ExprKind::Constant {
                            value: constant_piece.to_owned().into(),
                            kind: None,
                        }));
                        constant_piece.clear();
                    }
                    let parsed_expr = self.parse_fstring(nested + 1)?;
                    spec_constructor.extend(parsed_expr);
                    continue;
                }
                '}' => {
                    break;
                }
                _ => {
                    constant_piece.push(next);
                }
            }
            self.next_char();
        }
        if !constant_piece.is_empty() {
            spec_constructor.push(self.expr(ExprKind::Constant {
                value: constant_piece.to_owned().into(),
                kind: None,
            }));
            constant_piece.clear();
        }
        Ok(spec_constructor)
    }

    fn parse_fstring(&mut self, nested: u8) -> Result<Vec<Expr>, LexicalError> {
        if nested >= 2 {
            return Err(ExpressionNestedTooDeeply.to_lexical_error(self.get_pos()));
        }

        let mut content = String::new();
        let mut values = vec![];

        while let Some(&ch) = self.peek() {
            match ch {
                '{' => {
                    self.next_char();
                    if nested == 0 {
                        match self.peek() {
                            Some('{') => {
                                self.next_char();
                                content.push('{');
                                continue;
                            }
                            None => return Err(UnclosedLbrace.to_lexical_error(self.get_pos())),
                            _ => {}
                        }
                    }
                    if !content.is_empty() {
                        values.push(self.expr(ExprKind::Constant {
                            value: mem::take(&mut content).into(),
                            kind: None,
                        }));
                    }

                    let parsed_values = self.parse_formatted_value(nested)?;
                    values.extend(parsed_values);
                }
                '}' => {
                    if nested > 0 {
                        break;
                    }
                    self.next_char();
                    if let Some('}') = self.peek() {
                        self.next_char();
                        content.push('}');
                    } else {
                        return Err(SingleRbrace.to_lexical_error(self.get_pos()));
                    }
                }
                '\\' if !self.kind.is_raw() => {
                    self.next_char();
                    content.push_str(&self.parse_escaped_char()?);
                }
                _ => {
                    content.push(ch);
                    self.next_char();
                }
            }
        }

        if !content.is_empty() {
            values.push(self.expr(ExprKind::Constant {
                value: content.into(),
                kind: None,
            }))
        }

        Ok(values)
    }

    pub fn parse_bytes(&mut self) -> Result<Expr, LexicalError> {
        let mut content = String::new();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    content.push_str(&self.parse_escaped_char()?);
                }
                ch => {
                    if !ch.is_ascii() {
                        return Err(LexicalError {
                            error: LexicalErrorType::OtherError(
                                "bytes can only contain ASCII literal characters".to_string(),
                            ),
                            location: self.get_pos(),
                        });
                    }
                    content.push(ch);
                }
            }
        }

        Ok(self.expr(ExprKind::Constant {
            value: Constant::Bytes(content.chars().map(|c| c as u8).collect()),
            kind: None,
        }))
    }

    pub fn parse_string(&mut self) -> Result<Expr, LexicalError> {
        let mut content = String::new();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    content.push_str(&self.parse_escaped_char()?);
                }
                ch => content.push(ch),
            }
        }
        Ok(self.expr(ExprKind::Constant {
            value: Constant::Str(content),
            kind: self.kind.is_unicode().then(|| "u".to_string()),
        }))
    }

    pub fn parse(&mut self) -> Result<Vec<Expr>, LexicalError> {
        if self.kind.is_fstring() {
            self.parse_fstring(0)
        } else if self.kind.is_bytes() {
            self.parse_bytes().map(|expr| vec![expr])
        } else {
            self.parse_string().map(|expr| vec![expr])
        }
    }
}

fn parse_fstring_expr(source: &str, location: Location) -> Result<Expr, ParseError> {
    let fstring_body = format!("({source})");
    parse_expression_located(
        &fstring_body,
        "<fstring>",
        Location::new(location.row(), location.column() - 1),
    )
}

pub fn parse_string(
    source: &str,
    kind: StringKind,
    triple_quoted: bool,
    start: Location,
    end: Location,
) -> Result<Vec<Expr>, LexicalError> {
    StringParser::new(source, kind, triple_quoted, start, end).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_fstring(source: &str) -> Result<Vec<Expr>, FStringErrorType> {
        StringParser::new(
            source,
            StringKind::FString,
            false,
            Location::new(1, 0),
            Location::new(1, source.len() + 3), // 3 for prefix and quotes
        )
        .parse()
        .map_err(|e| match e.error {
            LexicalErrorType::FStringError(e) => e,
            e => unreachable!("Unexpected error type {:?}", e),
        })
    }

    #[test]
    fn test_parse_fstring() {
        let source = "{a}{ b }{{foo}}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_spec() {
        let source = "{foo:{spec}}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_not_nested_spec() {
        let source = "{foo:spec}";
        let parse_ast = parse_fstring(source).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_empty_fstring() {
        insta::assert_debug_snapshot!(parse_fstring("").unwrap());
    }

    #[test]
    fn test_fstring_parse_selfdocumenting_base() {
        let src = "{user=}";
        let parse_ast = parse_fstring(src).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_parse_selfdocumenting_base_more() {
        let src = "mix {user=} with text and {second=}";
        let parse_ast = parse_fstring(src).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_parse_selfdocumenting_format() {
        let src = "{user=:>10}";
        let parse_ast = parse_fstring(src).unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_invalid_fstring() {
        assert_eq!(parse_fstring("{5!a"), Err(UnclosedLbrace));
        assert_eq!(parse_fstring("{5!a1}"), Err(UnclosedLbrace));
        assert_eq!(parse_fstring("{5!"), Err(UnclosedLbrace));
        assert_eq!(parse_fstring("abc{!a 'cat'}"), Err(EmptyExpression));
        assert_eq!(parse_fstring("{!a"), Err(EmptyExpression));
        assert_eq!(parse_fstring("{ !a}"), Err(EmptyExpression));

        assert_eq!(parse_fstring("{5!}"), Err(InvalidConversionFlag));
        assert_eq!(parse_fstring("{5!x}"), Err(InvalidConversionFlag));

        assert_eq!(parse_fstring("{a:{a:{b}}}"), Err(ExpressionNestedTooDeeply));

        assert_eq!(parse_fstring("{a:b}}"), Err(SingleRbrace));
        assert_eq!(parse_fstring("}"), Err(SingleRbrace));
        assert_eq!(parse_fstring("{a:{b}"), Err(UnclosedLbrace));
        assert_eq!(parse_fstring("{"), Err(UnclosedLbrace));

        assert_eq!(parse_fstring("{}"), Err(EmptyExpression));

        // TODO: check for InvalidExpression enum?
        assert!(parse_fstring("{class}").is_err());
    }

    #[test]
    fn test_parse_fstring_not_equals() {
        let source = "{1 != 2}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_equals() {
        let source = "{42 == 42}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_selfdoc_prec_space() {
        let source = "{x   =}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_selfdoc_trailing_space() {
        let source = "{x=   }";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_yield_expr() {
        let source = "{yield}";
        let parse_ast = parse_fstring(source).unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
