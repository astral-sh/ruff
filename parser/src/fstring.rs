use self::FStringErrorType::*;
use crate::{
    ast::{Constant, ConversionFlag, Expr, ExprKind, Location},
    error::{FStringError, FStringErrorType, ParseError},
    parser::parse_expression,
};
use std::{iter, mem, str};

struct FStringParser {
    str_location: Location,
}

impl FStringParser {
    fn new(str_location: Location) -> Self {
        Self { str_location }
    }

    #[inline]
    fn expr(&self, node: ExprKind) -> Expr {
        Expr::new(self.str_location, node)
    }

    fn parse_formatted_value<'a>(
        &mut self,
        mut chars: iter::Peekable<str::Chars<'a>>,
        nested: u8,
    ) -> Result<(Vec<Expr>, iter::Peekable<str::Chars<'a>>), FStringErrorType> {
        let mut expression = String::from("{");
        let mut spec = None;
        let mut delims = Vec::new();
        let mut conversion = ConversionFlag::None;
        let mut self_documenting = false;
        let mut trailing_seq = String::new();

        while let Some(ch) = chars.next() {
            match ch {
                // can be integrated better with the remainign code, but as a starting point ok
                // in general I would do here a tokenizing of the fstrings to omit this peeking.
                '!' if chars.peek() == Some(&'=') => {
                    expression.push_str("!=");
                    chars.next();
                }

                '=' if chars.peek() == Some(&'=') => {
                    expression.push_str("==");
                    chars.next();
                }

                '>' if chars.peek() == Some(&'=') => {
                    expression.push_str(">=");
                    chars.next();
                }

                '<' if chars.peek() == Some(&'=') => {
                    expression.push_str("<=");
                    chars.next();
                }

                '!' if delims.is_empty() && chars.peek() != Some(&'=') => {
                    if expression.trim().is_empty() {
                        return Err(EmptyExpression);
                    }

                    conversion = match chars.next() {
                        Some('s') => ConversionFlag::Str,
                        Some('a') => ConversionFlag::Ascii,
                        Some('r') => ConversionFlag::Repr,
                        Some(_) => {
                            return Err(InvalidConversionFlag);
                        }
                        None => {
                            return Err(ExpectedRbrace);
                        }
                    };

                    if let Some(&peek) = chars.peek() {
                        if peek != '}' && peek != ':' {
                            if expression[1..].trim().is_empty() {
                                return Err(EmptyExpression);
                            } else {
                                return Err(ExpectedRbrace);
                            }
                        }
                    } else if expression[1..].trim().is_empty() {
                        return Err(EmptyExpression);
                    } else {
                        return Err(ExpectedRbrace);
                    }
                }

                // match a python 3.8 self documenting expression
                // format '{' PYTHON_EXPRESSION '=' FORMAT_SPECIFIER? '}'
                '=' if chars.peek() != Some(&'=') && delims.is_empty() => {
                    self_documenting = true;
                }

                ':' if delims.is_empty() => {
                    let (parsed_spec, remaining_chars) = self.parse_spec(chars, nested)?;

                    spec = Some(Box::new(self.expr(ExprKind::JoinedStr {
                        values: parsed_spec,
                    })));
                    chars = remaining_chars;
                }
                '(' | '{' | '[' => {
                    expression.push(ch);
                    delims.push(ch);
                }
                ')' => {
                    if delims.pop() != Some('(') {
                        return Err(MismatchedDelimiter);
                    }
                    expression.push(ch);
                }
                ']' => {
                    if delims.pop() != Some('[') {
                        return Err(MismatchedDelimiter);
                    }
                    expression.push(ch);
                }
                '}' if !delims.is_empty() => {
                    if delims.pop() != Some('{') {
                        return Err(MismatchedDelimiter);
                    }
                    expression.push(ch);
                }
                '}' => {
                    if expression[1..].trim().is_empty() {
                        return Err(EmptyExpression);
                    }
                    expression.push(ch);

                    let ret = if !self_documenting {
                        vec![self.expr(ExprKind::FormattedValue {
                            value: Box::new(
                                parse_fstring_expr(&expression[1..expression.len() - 1])
                                    .map_err(|e| InvalidExpression(Box::new(e.error)))?,
                            ),
                            conversion: conversion as _,
                            format_spec: spec,
                        })]
                    } else {
                        vec![
                            self.expr(ExprKind::Constant {
                                value: Constant::Str(
                                    expression[1..expression.len() - 1].to_owned() + "=",
                                ),
                                kind: None,
                            }),
                            self.expr(ExprKind::Constant {
                                value: trailing_seq.into(),
                                kind: None,
                            }),
                            self.expr(ExprKind::FormattedValue {
                                value: Box::new(
                                    parse_fstring_expr(&expression[1..expression.len() - 1])
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
                    return Ok((ret, chars));
                }
                '"' | '\'' => {
                    expression.push(ch);
                    for next in &mut chars {
                        expression.push(next);
                        if next == ch {
                            break;
                        }
                    }
                }
                ' ' if self_documenting => {
                    trailing_seq.push(ch);
                }
                _ => {
                    if self_documenting {
                        return Err(ExpectedRbrace);
                    }

                    expression.push(ch);
                }
            }
        }
        Err(UnclosedLbrace)
    }

    fn parse_spec<'a>(
        &mut self,
        mut chars: iter::Peekable<str::Chars<'a>>,
        nested: u8,
    ) -> Result<(Vec<Expr>, iter::Peekable<str::Chars<'a>>), FStringErrorType> {
        let mut spec_constructor = Vec::new();
        let mut constant_piece = String::new();
        while let Some(&next) = chars.peek() {
            match next {
                '{' => {
                    if !constant_piece.is_empty() {
                        spec_constructor.push(self.expr(ExprKind::Constant {
                            value: constant_piece.to_owned().into(),
                            kind: None,
                        }));
                        constant_piece.clear();
                    }
                    let (parsed_expr, remaining_chars) = self.parse(chars, nested + 1)?;
                    spec_constructor.extend(parsed_expr);
                    chars = remaining_chars;
                    continue;
                }
                '}' => {
                    break;
                }
                _ => {
                    constant_piece.push(next);
                }
            }
            chars.next();
        }
        if !constant_piece.is_empty() {
            spec_constructor.push(self.expr(ExprKind::Constant {
                value: constant_piece.to_owned().into(),
                kind: None,
            }));
            constant_piece.clear();
        }
        Ok((spec_constructor, chars))
    }

    fn parse<'a>(
        &mut self,
        mut chars: iter::Peekable<str::Chars<'a>>,
        nested: u8,
    ) -> Result<(Vec<Expr>, iter::Peekable<str::Chars<'a>>), FStringErrorType> {
        if nested >= 2 {
            return Err(ExpressionNestedTooDeeply);
        }

        let mut content = String::new();
        let mut values = vec![];

        while let Some(&ch) = chars.peek() {
            match ch {
                '{' => {
                    chars.next();
                    if nested == 0 {
                        if let Some('{') = chars.peek() {
                            chars.next();
                            content.push('{');
                            continue;
                        }
                    }
                    if !content.is_empty() {
                        values.push(self.expr(ExprKind::Constant {
                            value: mem::take(&mut content).into(),
                            kind: None,
                        }));
                    }

                    let (parsed_values, remaining_chars) =
                        self.parse_formatted_value(chars, nested)?;
                    values.extend(parsed_values);
                    chars = remaining_chars;
                }
                '}' => {
                    if nested > 0 {
                        break;
                    }
                    chars.next();
                    if let Some('}') = chars.peek() {
                        chars.next();
                        content.push('}');
                    } else {
                        return Err(UnopenedRbrace);
                    }
                }
                _ => {
                    content.push(ch);
                    chars.next();
                }
            }
        }

        if !content.is_empty() {
            values.push(self.expr(ExprKind::Constant {
                value: content.into(),
                kind: None,
            }))
        }

        Ok((values, chars))
    }
}

fn parse_fstring_expr(source: &str) -> Result<Expr, ParseError> {
    let fstring_body = format!("({})", source);
    parse_expression(&fstring_body, "<fstring>")
}

/// Parse an fstring from a string, located at a certain position in the sourcecode.
/// In case of errors, we will get the location and the error returned.
pub fn parse_located_fstring(source: &str, location: Location) -> Result<Vec<Expr>, FStringError> {
    FStringParser::new(location)
        .parse(source.chars().peekable(), 0)
        .map(|(e, _)| e)
        .map_err(|error| FStringError { error, location })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_fstring(source: &str) -> Result<Vec<Expr>, FStringErrorType> {
        FStringParser::new(Location::default())
            .parse(source.chars().peekable(), 0)
            .map(|(e, _)| e)
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
        assert_eq!(parse_fstring("{5!a"), Err(ExpectedRbrace));
        assert_eq!(parse_fstring("{5!a1}"), Err(ExpectedRbrace));
        assert_eq!(parse_fstring("{5!"), Err(ExpectedRbrace));
        assert_eq!(parse_fstring("abc{!a 'cat'}"), Err(EmptyExpression));
        assert_eq!(parse_fstring("{!a"), Err(EmptyExpression));
        assert_eq!(parse_fstring("{ !a}"), Err(EmptyExpression));

        assert_eq!(parse_fstring("{5!}"), Err(InvalidConversionFlag));
        assert_eq!(parse_fstring("{5!x}"), Err(InvalidConversionFlag));

        assert_eq!(parse_fstring("{a:{a:{b}}}"), Err(ExpressionNestedTooDeeply));

        assert_eq!(parse_fstring("{a:b}}"), Err(UnopenedRbrace));
        assert_eq!(parse_fstring("}"), Err(UnopenedRbrace));
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
