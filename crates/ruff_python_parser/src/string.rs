//! Parsing of string literals, bytes literals, and implicit string concatenation.

use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::lexer::{LexicalError, LexicalErrorType};
use crate::token::{StringKind, Tok};

pub(crate) enum StringType {
    Str(ast::StringLiteral),
    Bytes(ast::BytesLiteral),
    FString(ast::FString),
}

impl Ranged for StringType {
    fn range(&self) -> TextRange {
        match self {
            Self::Str(node) => node.range(),
            Self::Bytes(node) => node.range(),
            Self::FString(node) => node.range(),
        }
    }
}

impl From<StringType> for Expr {
    fn from(string: StringType) -> Self {
        match string {
            StringType::Str(node) => Expr::from(node),
            StringType::Bytes(node) => Expr::from(node),
            StringType::FString(node) => Expr::from(node),
        }
    }
}

struct StringParser<'a> {
    rest: &'a str,
    kind: StringKind,
    location: TextSize,
    range: TextRange,
}

impl<'a> StringParser<'a> {
    fn new(source: &'a str, kind: StringKind, start: TextSize, range: TextRange) -> Self {
        Self {
            rest: source,
            kind,
            location: start,
            range,
        }
    }

    #[inline]
    fn skip_bytes(&mut self, bytes: usize) -> &'a str {
        let skipped_str = &self.rest[..bytes];
        self.rest = &self.rest[bytes..];
        self.location += skipped_str.text_len();
        skipped_str
    }

    #[inline]
    fn get_pos(&self) -> TextSize {
        self.location
    }

    /// Returns the next byte in the string, if there is one.
    ///
    /// # Panics
    ///
    /// When the next byte is a part of a multi-byte character.
    #[inline]
    fn next_byte(&mut self) -> Option<u8> {
        self.rest.as_bytes().first().map(|&byte| {
            self.rest = &self.rest[1..];
            self.location += TextSize::new(1);
            byte
        })
    }

    #[inline]
    fn next_char(&mut self) -> Option<char> {
        self.rest.chars().next().map(|c| {
            self.rest = &self.rest[c.len_utf8()..];
            self.location += c.text_len();
            c
        })
    }

    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.rest.as_bytes().first().copied()
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
    fn parse_octet(&mut self, o: u8) -> char {
        let mut radix_bytes = [o, 0, 0];
        let mut len = 1;

        while len < 3 {
            let Some(b'0'..=b'7') = self.peek_byte() else {
                break;
            };

            radix_bytes[len] = self.next_byte().unwrap();
            len += 1;
        }

        // SAFETY: radix_bytes is always going to be in the ASCII range.
        #[allow(unsafe_code)]
        let radix_str = unsafe { std::str::from_utf8_unchecked(&radix_bytes[..len]) };

        let value = u32::from_str_radix(radix_str, 8).unwrap();
        char::from_u32(value).unwrap()
    }

    fn parse_unicode_name(&mut self) -> Result<char, LexicalError> {
        let start_pos = self.get_pos();

        let Some('{') = self.next_char() else {
            return Err(LexicalError::new(LexicalErrorType::StringError, start_pos));
        };

        let start_pos = self.get_pos();
        let Some(close_idx) = self.rest.find('}') else {
            return Err(LexicalError::new(
                LexicalErrorType::StringError,
                self.get_pos(),
            ));
        };

        let name_and_ending = self.skip_bytes(close_idx + 1);
        let name = &name_and_ending[..name_and_ending.len() - 1];

        unicode_names2::character(name)
            .ok_or_else(|| LexicalError::new(LexicalErrorType::UnicodeError, start_pos))
    }

    fn parse_escaped_char(&mut self, string: &mut String) -> Result<(), LexicalError> {
        let Some(first_char) = self.next_char() else {
            return Err(LexicalError {
                error: LexicalErrorType::StringError,
                location: self.get_pos(),
            });
        };

        let new_char = match first_char {
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
            o @ '0'..='7' => self.parse_octet(o as u8),
            'x' => self.parse_unicode_literal(2)?,
            'u' if !self.kind.is_any_bytes() => self.parse_unicode_literal(4)?,
            'U' if !self.kind.is_any_bytes() => self.parse_unicode_literal(8)?,
            'N' if !self.kind.is_any_bytes() => self.parse_unicode_name()?,
            // Special cases where the escape sequence is not a single character
            '\n' => return Ok(()),
            '\r' => {
                if self.peek_byte() == Some(b'\n') {
                    self.next_byte();
                }

                return Ok(());
            }
            _ => {
                if self.kind.is_any_bytes() && !first_char.is_ascii() {
                    return Err(LexicalError {
                        error: LexicalErrorType::OtherError(
                            "bytes can only contain ASCII literal characters".to_owned(),
                        ),
                        location: self.get_pos(),
                    });
                }

                string.push('\\');

                first_char
            }
        };

        string.push(new_char);

        Ok(())
    }

    fn parse_fstring_middle(&mut self) -> Result<Expr, LexicalError> {
        let mut value = String::new();
        while let Some(ch) = self.next_char() {
            match ch {
                // We can encounter a `\` as the last character in a `FStringMiddle`
                // token which is valid in this context. For example,
                //
                // ```python
                // f"\{foo} \{bar:\}"
                // # ^     ^^     ^
                // ```
                //
                // Here, the `FStringMiddle` token content will be "\" and " \"
                // which is invalid if we look at the content in isolation:
                //
                // ```python
                // "\"
                // ```
                //
                // However, the content is syntactically valid in the context of
                // the f-string because it's a substring of the entire f-string.
                // This is still an invalid escape sequence, but we don't want to
                // raise a syntax error as is done by the CPython parser. It might
                // be supported in the future, refer to point 3: https://peps.python.org/pep-0701/#rejected-ideas
                '\\' if !self.kind.is_raw() && self.peek_byte().is_some() => {
                    self.parse_escaped_char(&mut value)?;
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
        Ok(Expr::from(ast::StringLiteral {
            value,
            unicode: false,
            range: self.range,
        }))
    }

    fn parse_bytes(&mut self) -> Result<StringType, LexicalError> {
        let mut content = String::new();
        while let Some(ch) = self.next_char() {
            match ch {
                '\\' if !self.kind.is_raw() => {
                    self.parse_escaped_char(&mut content)?;
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

        Ok(StringType::Bytes(ast::BytesLiteral {
            value: content.chars().map(|c| c as u8).collect::<Vec<u8>>(),
            range: self.range,
        }))
    }

    fn parse_string(&mut self) -> Result<StringType, LexicalError> {
        let mut value = String::new();

        if self.kind.is_raw() {
            value.push_str(self.skip_bytes(self.rest.len()));
        } else {
            loop {
                let Some(escape_idx) = self.rest.find('\\') else {
                    value.push_str(self.skip_bytes(self.rest.len()));
                    break;
                };

                let before_with_slash = self.skip_bytes(escape_idx + 1);
                let before = &before_with_slash[..before_with_slash.len() - 1];

                value.push_str(before);
                self.parse_escaped_char(&mut value)?;
            }
        }
        Ok(StringType::Str(ast::StringLiteral {
            value,
            unicode: self.kind.is_unicode(),
            range: self.range,
        }))
    }

    fn parse(&mut self) -> Result<StringType, LexicalError> {
        if self.kind.is_any_bytes() {
            self.parse_bytes()
        } else {
            self.parse_string()
        }
    }
}

pub(crate) fn parse_string_literal(
    source: &str,
    kind: StringKind,
    triple_quoted: bool,
    range: TextRange,
) -> Result<StringType, LexicalError> {
    let start_location = range.start()
        + kind.prefix_len()
        + if triple_quoted {
            TextSize::from(3)
        } else {
            TextSize::from(1)
        };
    StringParser::new(source, kind, start_location, range).parse()
}

pub(crate) fn parse_fstring_middle(
    source: &str,
    is_raw: bool,
    range: TextRange,
) -> Result<Expr, LexicalError> {
    let kind = if is_raw {
        StringKind::RawString
    } else {
        StringKind::String
    };
    StringParser::new(source, kind, range.start(), range).parse_fstring_middle()
}

pub(crate) fn concatenated_strings(
    strings: Vec<StringType>,
    range: TextRange,
) -> Result<Expr, LexicalError> {
    #[cfg(debug_assertions)]
    debug_assert!(strings.len() > 1);

    let mut has_fstring = false;
    let mut byte_literal_count = 0;
    for string in &strings {
        match string {
            StringType::FString(_) => has_fstring = true,
            StringType::Bytes(_) => byte_literal_count += 1,
            StringType::Str(_) => {}
        }
    }
    let has_bytes = byte_literal_count > 0;

    if has_bytes && byte_literal_count < strings.len() {
        return Err(LexicalError {
            error: LexicalErrorType::OtherError(
                "cannot mix bytes and nonbytes literals".to_owned(),
            ),
            location: range.start(),
        });
    }

    if has_bytes {
        let mut values = Vec::with_capacity(strings.len());
        for string in strings {
            match string {
                StringType::Bytes(value) => values.push(value),
                _ => unreachable!("Unexpected non-bytes literal."),
            }
        }
        return Ok(Expr::from(ast::ExprBytesLiteral {
            value: ast::BytesLiteralValue::concatenated(values),
            range,
        }));
    }

    if !has_fstring {
        let mut values = Vec::with_capacity(strings.len());
        for string in strings {
            match string {
                StringType::Str(value) => values.push(value),
                _ => unreachable!("Unexpected non-string literal."),
            }
        }
        return Ok(Expr::from(ast::ExprStringLiteral {
            value: ast::StringLiteralValue::concatenated(values),
            range,
        }));
    }

    let mut parts = Vec::with_capacity(strings.len());
    for string in strings {
        match string {
            StringType::FString(fstring) => parts.push(ast::FStringPart::FString(fstring)),
            StringType::Str(string) => parts.push(ast::FStringPart::Literal(string)),
            StringType::Bytes(_) => unreachable!("Unexpected bytes literal."),
        }
    }

    Ok(ast::ExprFString {
        value: ast::FStringValue::concatenated(parts),
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
    // TODO(dhruvmanila): The parser can't catch all cases of this error, but
    // wherever it can, we'll display the correct error message.
    /// A lambda expression without parentheses was encountered.
    LambdaWithoutParentheses,
}

impl std::fmt::Display for FStringErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use FStringErrorType::{
            InvalidConversionFlag, LambdaWithoutParentheses, SingleRbrace, UnclosedLbrace,
            UnterminatedString, UnterminatedTripleQuotedString,
        };
        match self {
            UnclosedLbrace => write!(f, "expecting '}}'"),
            InvalidConversionFlag => write!(f, "invalid conversion character"),
            SingleRbrace => write!(f, "single '}}' is not allowed"),
            UnterminatedString => write!(f, "unterminated string"),
            UnterminatedTripleQuotedString => write!(f, "unterminated triple-quoted string"),
            LambdaWithoutParentheses => {
                write!(f, "lambda expressions are not allowed without parentheses")
            }
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
    use crate::{ParseErrorType, Suite};

    use super::*;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    fn string_parser_escaped_eol(eol: &str) -> Suite {
        let source = format!(r"'text \{eol}more text'");
        parse_suite(&source, "<test>").unwrap()
    }

    #[test]
    fn test_string_parser_escaped_unix_eol() {
        let parse_ast = string_parser_escaped_eol(UNIX_EOL);
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_string_parser_escaped_mac_eol() {
        let parse_ast = string_parser_escaped_eol(MAC_EOL);
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_string_parser_escaped_windows_eol() {
        let parse_ast = string_parser_escaped_eol(WINDOWS_EOL);
        insta::assert_debug_snapshot!(parse_ast);
    }

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
        use FStringErrorType::{InvalidConversionFlag, LambdaWithoutParentheses};

        assert_eq!(parse_fstring_error(r#"f"{5!x}""#), InvalidConversionFlag);
        assert_eq!(
            parse_fstring_error("f'{lambda x:{x}}'"),
            LambdaWithoutParentheses
        );
        assert_eq!(
            parse_fstring_error("f'{lambda x: {x}}'"),
            LambdaWithoutParentheses
        );
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

    /// <https://github.com/astral-sh/ruff/issues/8355>
    #[test]
    fn test_dont_panic_on_8_in_octal_escape() {
        let source = r"bold = '\038[1m'";
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
