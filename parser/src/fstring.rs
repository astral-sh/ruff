// We no longer need this file
use self::FStringErrorType::*;
use crate::{
    ast::{Constant, ConversionFlag, Expr, ExprKind, Location},
    error::{FStringError, FStringErrorType, ParseError},
    parser::parse_expression,
};
use std::{iter, mem, str};

struct FStringParser<'a> {
    chars: iter::Peekable<str::Chars<'a>>,
    str_start: Location,
    str_end: Location,
}

impl<'a> FStringParser<'a> {
    fn new(source: &'a str, str_start: Location, str_end: Location) -> Self {
        Self {
            chars: source.chars().peekable(),
            str_start,
            str_end,
        }
    }

    fn next_char(&mut self) -> Option<char> {
        self.chars.next()
    }

    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    #[inline]
    fn expr(&self, node: ExprKind) -> Expr {
        Expr::new(self.str_start, self.str_end, node)
    }

    fn parse_formatted_value(&mut self, nested: u8) -> Result<Vec<Expr>, FStringErrorType> {
        let mut expression = String::new();
        let mut spec = None;
        let mut delims = Vec::new();
        let mut conversion = ConversionFlag::None;
        let mut self_documenting = false;
        let mut trailing_seq = String::new();

        while let Some(ch) = self.next_char() {
            match ch {
                // can be integrated better with the remainign code, but as a starting point ok
                // in general I would do here a tokenizing of the fstrings to omit this peeking.
                '!' if self.peek() == Some(&'=') => {
                    expression.push_str("!=");
                    self.next_char();
                }

                '=' if self.peek() == Some(&'=') => {
                    expression.push_str("==");
                    self.next_char();
                }

                '>' if self.peek() == Some(&'=') => {
                    expression.push_str(">=");
                    self.next_char();
                }

                '<' if self.peek() == Some(&'=') => {
                    expression.push_str("<=");
                    self.next_char();
                }

                '!' if delims.is_empty() && self.peek() != Some(&'=') => {
                    if expression.trim().is_empty() {
                        return Err(EmptyExpression);
                    }

                    conversion = match self.next_char() {
                        Some('s') => ConversionFlag::Str,
                        Some('a') => ConversionFlag::Ascii,
                        Some('r') => ConversionFlag::Repr,
                        Some(_) => {
                            return Err(if expression.trim().is_empty() {
                                EmptyExpression
                            } else {
                                InvalidConversionFlag
                            });
                        }
                        None => {
                            return Err(if expression.trim().is_empty() {
                                EmptyExpression
                            } else {
                                UnclosedLbrace
                            });
                        }
                    };

                    if let Some(&peek) = self.peek() {
                        if peek != '}' && peek != ':' {
                            return Err(if expression.trim().is_empty() {
                                EmptyExpression
                            } else {
                                UnclosedLbrace
                            });
                        }
                    } else {
                        return Err(if expression.trim().is_empty() {
                            EmptyExpression
                        } else {
                            UnclosedLbrace
                        });
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
                            return Err(MismatchedDelimiter(c, ')'));
                        }
                        None => {
                            return Err(Unmatched(')'));
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
                            return Err(MismatchedDelimiter(c, ']'));
                        }
                        None => {
                            return Err(Unmatched(']'));
                        }
                    }
                }
                '}' if !delims.is_empty() => {
                    let last_delim = delims.pop();
                    match last_delim {
                        Some('{') => {
                            expression.push(ch);
                        }
                        Some(c) => return Err(MismatchedDelimiter(c, '}')),
                        None => {}
                    }
                }
                '}' => {
                    if expression.trim().is_empty() {
                        return Err(EmptyExpression);
                    }

                    let ret = if !self_documenting {
                        vec![self.expr(ExprKind::FormattedValue {
                            value: Box::new(
                                parse_fstring_expr(&expression)
                                    .map_err(|e| InvalidExpression(Box::new(e.error)))?,
                            ),
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
                                    parse_fstring_expr(&expression)
                                        .map_err(|e| InvalidExpression(Box::new(e.error)))?,
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
                            return Err(UnterminatedString);
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
                '\\' => return Err(ExpressionCannotInclude('\\')),
                _ => {
                    if self_documenting {
                        return Err(UnclosedLbrace);
                    }

                    expression.push(ch);
                }
            }
        }
        Err(if expression.trim().is_empty() {
            EmptyExpression
        } else {
            UnclosedLbrace
        })
    }

    fn parse_spec(&mut self, nested: u8) -> Result<Vec<Expr>, FStringErrorType> {
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
                    let parsed_expr = self.parse(nested + 1)?;
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

    fn parse(&mut self, nested: u8) -> Result<Vec<Expr>, FStringErrorType> {
        if nested >= 2 {
            return Err(ExpressionNestedTooDeeply);
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
                            None => return Err(UnclosedLbrace),
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
                        return Err(SingleRbrace);
                    }
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
}

fn parse_fstring_expr(source: &str) -> Result<Expr, ParseError> {
    let fstring_body = format!("({source})");
    parse_expression(&fstring_body, "<fstring>")
}

/// Parse an fstring from a string, located at a certain position in the sourcecode.
/// In case of errors, we will get the location and the error returned.
pub fn parse_located_fstring(
    source: &str,
    start: Location,
    end: Location,
) -> Result<Vec<Expr>, FStringError> {
    FStringParser::new(source, start, end)
        .parse(0)
        .map_err(|error| FStringError {
            error,
            location: start,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_fstring(source: &str) -> Result<Vec<Expr>, FStringErrorType> {
        FStringParser::new(source, Location::default(), Location::default()).parse(0)
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
