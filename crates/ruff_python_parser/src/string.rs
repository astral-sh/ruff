//! Contains the logic for parsing string and bytes literals, as well as
//! implicit string concatenation.

use ruff_python_ast::{
    self as ast, BytesConstant, Constant, Expr, ParenthesizedExpr, StringConstant,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::lexer::{LexicalError, LexicalErrorType};
use crate::token::{StringKind, Tok};

// unicode_name2 does not expose `MAX_NAME_LENGTH`, so we replicate that constant here, fix #3798
const MAX_UNICODE_NAME: usize = 88;

struct StringParser<'a> {
    chars: std::str::Chars<'a>,
    kind: StringKind,
    location: TextSize,
}

impl<'a> StringParser<'a> {
    fn new(source: &'a str, kind: StringKind, start: TextSize) -> Self {
        Self {
            chars: source.chars(),
            kind,
            location: start,
        }
    }

    #[inline]
    fn next_char(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.location += c.text_len();
        Some(c)
    }

    #[inline]
    fn peek(&mut self) -> Option<char> {
        self.chars.clone().next()
    }

    #[inline]
    fn get_pos(&self) -> TextSize {
        self.location
    }

    #[inline]
    fn range(&self, start_location: TextSize) -> TextRange {
        TextRange::new(start_location, self.location)
    }

    fn parse_unicode_literal(&mut self, literal_number: usize) -> Result<char, LexicalError> {
        let mut p: u32 = 0u32;
        let unicode_error = LexicalError::new(LexicalErrorType::UnicodeError, self.get_pos());
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
                octet_content.push(self.next_char().unwrap());
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
            _ => return Err(LexicalError::new(LexicalErrorType::StringError, start_pos)),
        }
        let start_pos = self.get_pos();
        let mut name = String::new();
        loop {
            match self.next_char() {
                Some('}') => break,
                Some(c) => name.push(c),
                None => {
                    return Err(LexicalError::new(
                        LexicalErrorType::StringError,
                        self.get_pos(),
                    ))
                }
            }
        }

        if name.len() > MAX_UNICODE_NAME {
            return Err(LexicalError::new(
                LexicalErrorType::UnicodeError,
                self.get_pos(),
            ));
        }

        unicode_names2::character(&name)
            .ok_or_else(|| LexicalError::new(LexicalErrorType::UnicodeError, start_pos))
    }

    fn parse_escaped_char(&mut self) -> Result<String, LexicalError> {
        match self.next_char() {
            Some(c) => {
                let char = match c {
                    '\\' => '\\',
                    '\'' => '\'',
                    '\"' => '"',
                    'a' => '\x07',
                    'b' => '\x08',
                    'f' => '\x0c',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'v' => '\x0b',
                    o @ '0'..='7' => self.parse_octet(o),
                    'x' => self.parse_unicode_literal(2)?,
                    'u' if !self.kind.is_any_bytes() => self.parse_unicode_literal(4)?,
                    'U' if !self.kind.is_any_bytes() => self.parse_unicode_literal(8)?,
                    'N' if !self.kind.is_any_bytes() => self.parse_unicode_name()?,
                    // Special cases where the escape sequence is not a single character
                    '\n' => return Ok(String::new()),
                    c => {
                        if self.kind.is_any_bytes() && !c.is_ascii() {
                            return Err(LexicalError {
                                error: LexicalErrorType::OtherError(
                                    "bytes can only contain ASCII literal characters".to_owned(),
                                ),
                                location: self.get_pos(),
                            });
                        }
                        return Ok(format!("\\{c}"));
                    }
                };
                Ok(char.to_string())
            }
            None => Err(LexicalError {
                error: LexicalErrorType::StringError,
                location: self.get_pos(),
            }),
        }
    }

    fn parse_fstring_middle(&mut self) -> Result<Expr, LexicalError> {
        let mut value = String::new();
        let start_location = self.get_pos();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    value.push_str(&self.parse_escaped_char()?);
                }
                // If there are any curly braces inside a `FStringMiddle` token,
                // then they were escaped (i.e. `{{` or `}}`). This means that
                // we need increase the location by 2 instead of 1.
                ch @ ('{' | '}') => {
                    self.location += ch.text_len();
                    value.push(ch);
                }
                ch => value.push(ch),
            }
        }
        Ok(Expr::from(ast::ExprConstant {
            value: value.into(),
            range: self.range(start_location),
        }))
    }

    fn parse_bytes(&mut self) -> Result<Expr, LexicalError> {
        let mut content = String::new();
        let start_location = self.get_pos();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    content.push_str(&self.parse_escaped_char()?);
                }
                ch => {
                    if !ch.is_ascii() {
                        return Err(LexicalError::new(
                            LexicalErrorType::OtherError(
                                "bytes can only contain ASCII literal characters".to_string(),
                            ),
                            self.get_pos(),
                        ));
                    }
                    content.push(ch);
                }
            }
        }

        Ok(Expr::from(ast::ExprConstant {
            value: content.chars().map(|c| c as u8).collect::<Vec<u8>>().into(),
            range: self.range(start_location),
        }))
    }

    fn parse_string(&mut self) -> Result<Expr, LexicalError> {
        let mut value = String::new();
        let start_location = self.get_pos();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    value.push_str(&self.parse_escaped_char()?);
                }
                ch => value.push(ch),
            }
        }
        Ok(Expr::from(ast::ExprConstant {
            value: ast::Constant::Str(ast::StringConstant {
                value,
                unicode: self.kind.is_unicode(),
                implicit_concatenated: false,
            }),
            range: self.range(start_location),
        }))
    }

    fn parse(&mut self) -> Result<Expr, LexicalError> {
        if self.kind.is_any_bytes() {
            self.parse_bytes()
        } else {
            self.parse_string()
        }
    }
}

pub(crate) fn parse_string_literal(
    start: TextSize,
    (source, kind, triple_quoted): (String, StringKind, bool),
) -> Result<Expr, LexicalError> {
    let start = start
        + kind.prefix_len()
        + if triple_quoted {
            TextSize::from(3)
        } else {
            TextSize::from(1)
        };
    StringParser::new(source.as_str(), kind, start).parse()
}

pub(crate) fn parse_fstring_middle(
    start: TextSize,
    (source, is_raw): (String, bool),
) -> Result<Option<Expr>, LexicalError> {
    // This is to account for the empty `FStringMiddle` token that is created
    // to check for non-parenthesized lambda expressions.
    if source.is_empty() {
        return Ok(None);
    }
    let kind = if is_raw {
        StringKind::RawString
    } else {
        StringKind::String
    };
    StringParser::new(source.as_str(), kind, start)
        .parse_fstring_middle()
        .map(Some)
}

/// Concatenate a list of string expressions into a single string expression.
///
/// This is mainly used for implicit string concatenation and the possible
/// expression values are strings, bytes, and f-strings.
pub(crate) fn concatenate_strings(
    strings: Vec<ParenthesizedExpr>,
    range: TextRange,
) -> Result<ParenthesizedExpr, LexicalError> {
    #[cfg(debug_assertions)]
    debug_assert!(!strings.is_empty());

    let has_fstring = strings
        .iter()
        .any(|parenthesized_expr| parenthesized_expr.expr.is_f_string_expr());
    let num_bytes = strings
        .iter()
        .filter(|parenthesized_expr| {
            matches!(
                parenthesized_expr.expr,
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Bytes(_),
                    ..
                })
            )
        })
        .count();
    let has_bytes = num_bytes > 0;
    let implicit_concatenated = strings.len() > 1;

    if has_bytes && num_bytes < strings.len() {
        return Err(LexicalError {
            error: LexicalErrorType::OtherError(
                "cannot mix bytes and nonbytes literals".to_owned(),
            ),
            location: range.start(),
        });
    }

    if has_bytes {
        let mut content: Vec<u8> = vec![];
        for parenthesized_expr in strings {
            match parenthesized_expr.expr {
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Bytes(BytesConstant { value, .. }),
                    ..
                }) => content.extend(value),
                _ => unreachable!("Unexpected non-bytes expression."),
            }
        }
        return Ok(ast::ExprConstant {
            value: Constant::Bytes(BytesConstant {
                value: content,
                implicit_concatenated,
            }),
            range,
        }
        .into());
    }

    if !has_fstring {
        let mut content: Vec<String> = vec![];
        let is_unicode = match strings.get(0) {
            Some(ParenthesizedExpr {
                expr: Expr::Constant(..),
                ..
            }) => expr.is_unicode_string(),
            _ => false,
        };
        for parenthesized_expr in strings {
            match parenthesized_expr.expr {
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(StringConstant { value, .. }),
                    ..
                }) => content.push(value),
                _ => unreachable!("Unexpected non-string expression."),
            }
        }
        return Ok(ast::ExprConstant {
            value: Constant::Str(StringConstant {
                value: content.join(""),
                unicode: is_unicode,
                implicit_concatenated,
            }),
            range,
        }
        .into());
    }

    // De-duplicate adjacent constants.
    let mut deduped: Vec<Expr> = vec![];
    let mut current: Vec<String> = vec![];
    let mut current_start = range.start();
    let mut current_end = range.end();
    let mut is_unicode = false;

    let take_current = |current: &mut Vec<String>, start, end| -> Expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(StringConstant {
                value: current.drain(..).collect::<String>(),
                unicode: is_unicode,
                implicit_concatenated,
            }),
            range: TextRange::new(start, end),
        })
    };

    for parenthesized_expr in strings {
        let expr_range = parenthesized_expr.range();
        match parenthesized_expr.expr {
            Expr::FString(ast::ExprFString { values, .. }) => {
                for value in values {
                    let value_range = value.range();
                    match value {
                        Expr::FormattedValue { .. } => {
                            if !current.is_empty() {
                                deduped.push(take_current(
                                    &mut current,
                                    current_start,
                                    current_end,
                                ));
                            }
                            deduped.push(value);
                            is_unicode = false;
                        }
                        Expr::Constant(ast::ExprConstant {
                            value: Constant::Str(StringConstant { value, unicode, .. }),
                            ..
                        }) => {
                            if current.is_empty() {
                                is_unicode |= unicode;
                                current_start = value_range.start();
                            }
                            current_end = value_range.end();
                            current.push(value);
                        }
                        _ => unreachable!("Unexpected non-string expression."),
                    }
                }
            }
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(StringConstant { value, unicode, .. }),
                ..
            }) => {
                if current.is_empty() {
                    is_unicode |= unicode;
                    current_start = expr_range.start();
                }
                current_end = expr_range.end();
                current.push(value);
            }
            _ => unreachable!("Unexpected non-string expression."),
        }
    }
    if !current.is_empty() {
        deduped.push(take_current(&mut current, current_start, current_end));
    }

    Ok(ast::ExprFString {
        values: deduped,
        implicit_concatenated,
        range,
    }
    .into())
}

// TODO: consolidate these with ParseError
/// An error that occurred during parsing of an f-string.
#[derive(Debug, PartialEq)]
struct FStringError {
    /// The type of error that occurred.
    pub(crate) error: FStringErrorType,
    /// The location of the error.
    pub(crate) location: TextSize,
}

impl From<FStringError> for LexicalError {
    fn from(err: FStringError) -> Self {
        LexicalError {
            error: LexicalErrorType::FStringError(err.error),
            location: err.location,
        }
    }
}

/// Represents the different types of errors that can occur during parsing of an f-string.
#[derive(Debug, PartialEq)]
pub enum FStringErrorType {
    /// Expected a right brace after an opened left brace.
    UnclosedLbrace,
    /// An invalid conversion flag was encountered.
    InvalidConversionFlag,
    /// A single right brace was encountered.
    SingleRbrace,
    /// Unterminated string.
    UnterminatedString,
    /// Unterminated triple-quoted string.
    UnterminatedTripleQuotedString,
}

impl std::fmt::Display for FStringErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use FStringErrorType::{
            InvalidConversionFlag, SingleRbrace, UnclosedLbrace, UnterminatedString,
            UnterminatedTripleQuotedString,
        };
        match self {
            UnclosedLbrace => write!(f, "expecting '}}'"),
            InvalidConversionFlag => write!(f, "invalid conversion character"),
            SingleRbrace => write!(f, "single '}}' is not allowed"),
            UnterminatedString => write!(f, "unterminated string"),
            UnterminatedTripleQuotedString => write!(f, "unterminated triple-quoted string"),
        }
    }
}

impl From<FStringError> for crate::parser::LalrpopError<TextSize, Tok, LexicalError> {
    fn from(err: FStringError) -> Self {
        lalrpop_util::ParseError::User {
            error: LexicalError {
                error: LexicalErrorType::FStringError(err.error),
                location: err.location,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::LexicalErrorType;
    use crate::parser::parse_suite;
    use crate::ParseErrorType;

    use super::*;

    #[test]
    fn test_parse_fstring() {
        let source = r#"f"{a}{ b }{{foo}}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_spec() {
        let source = r#"f"{foo:{spec}}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_not_nested_spec() {
        let source = r#"f"{foo:spec}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_empty_fstring() {
        insta::assert_debug_snapshot!(parse_suite(r#"f"""#, "<test>").unwrap());
    }

    #[test]
    fn test_fstring_parse_self_documenting_base() {
        let source = r#"f"{user=}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_parse_self_documenting_base_more() {
        let source = r#"f"mix {user=} with text and {second=}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_parse_self_documenting_format() {
        let source = r#"f"{user=:>10}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    fn parse_fstring_error(source: &str) -> FStringErrorType {
        parse_suite(source, "<test>")
            .map_err(|e| match e.error {
                ParseErrorType::Lexical(LexicalErrorType::FStringError(e)) => e,
                e => unreachable!("Expected FStringError: {:?}", e),
            })
            .expect_err("Expected error")
    }

    #[test]
    fn test_parse_invalid_fstring() {
        use FStringErrorType::InvalidConversionFlag;

        assert_eq!(parse_fstring_error(r#"f"{5!x}""#), InvalidConversionFlag);
        assert!(parse_suite(r#"f"{class}""#, "<test>").is_err());
    }

    #[test]
    fn test_parse_fstring_not_equals() {
        let source = r#"f"{1 != 2}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_equals() {
        let source = r#"f"{42 == 42}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_self_doc_prec_space() {
        let source = r#"f"{x   =}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_self_doc_trailing_space() {
        let source = r#"f"{x=   }""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_yield_expr() {
        let source = r#"f"{yield}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string_concat() {
        let source = "'Hello ' 'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_1() {
        let source = "'Hello ' u'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_string_concat_2() {
        let source = "u'Hello ' 'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_1() {
        let source = "'Hello ' f'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_2() {
        let source = "'Hello ' f'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_3() {
        let source = "'Hello ' f'world{\"!\"}'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_f_string_concat_4() {
        let source = "'Hello ' f'world{\"!\"}' 'again!'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_1() {
        let source = "u'Hello ' f'world'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_u_f_string_concat_2() {
        let source = "u'Hello ' f'world' '!'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_string_triple_quotes_with_kind() {
        let source = "u'''Hello, world!'''";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_single_quoted_byte() {
        // single quote
        let source = r##"b'\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff'"##;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_double_quoted_byte() {
        // double quote
        let source = r##"b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff""##;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_escape_char_in_byte_literal() {
        // backslash does not escape
        let source = r#"b"omkmok\Xaa""#; // spell-checker:ignore omkmok
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_byte_literal_1() {
        let source = r"rb'\x1z'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_byte_literal_2() {
        let source = r"rb'\\'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_escape_octet() {
        let source = r"b'\43a\4\1234'";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_escaped_newline() {
        let source = r#"f"\n{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_constant_range() {
        let source = r#"f"aaa{bbb}ccc{ddd}eee""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_unescaped_newline() {
        let source = r#"f"""
{x}""""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_escaped_character() {
        let source = r#"f"\\{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_raw_fstring() {
        let source = r#"rf"{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_triple_quoted_raw_fstring() {
        let source = r#"rf"""{x}""""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_fstring_line_continuation() {
        let source = r#"rf"\
{x}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_string_spec() {
        let source = r#"f"{foo:{''}}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_parse_fstring_nested_concatenation_string_spec() {
        let source = r#"f"{foo:{'' ''}}""#;
        let parse_ast = parse_suite(source, "<test>").unwrap();

        insta::assert_debug_snapshot!(parse_ast);
    }

    macro_rules! test_aliases_parse {
        ($($name:ident: $alias:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let source = format!(r#""\N{{{0}}}""#, $alias);
                let parse_ast = parse_suite(&source, "<test>").unwrap();
                insta::assert_debug_snapshot!(parse_ast);
            }
        )*
        }
    }

    test_aliases_parse! {
        test_backspace_alias: "BACKSPACE",
        test_bell_alias: "BEL",
        test_carriage_return_alias: "CARRIAGE RETURN",
        test_delete_alias: "DELETE",
        test_escape_alias: "ESCAPE",
        test_form_feed_alias: "FORM FEED",
        test_hts_alias: "HTS",
        test_character_tabulation_with_justification_alias: "CHARACTER TABULATION WITH JUSTIFICATION",
    }
}
