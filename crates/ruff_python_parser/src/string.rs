//! Parsing of string literals, bytes literals, and implicit string concatenation.

use bstr::ByteSlice;

use ruff_python_ast::{self as ast, AnyStringFlags, Expr, StringFlags};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::error::{LexicalError, LexicalErrorType};

#[derive(Debug)]
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

enum EscapedChar {
    Literal(char),
    Escape(char),
}

struct StringParser {
    /// The raw content of the string e.g., the `foo` part in `"foo"`.
    source: Box<str>,
    /// Current position of the parser in the source.
    cursor: usize,
    /// Flags that can be used to query information about the string.
    flags: AnyStringFlags,
    /// The location of the first character in the source from the start of the file.
    offset: TextSize,
    /// The range of the string literal.
    range: TextRange,
}

impl StringParser {
    fn new(source: Box<str>, flags: AnyStringFlags, offset: TextSize, range: TextRange) -> Self {
        Self {
            source,
            cursor: 0,
            flags,
            offset,
            range,
        }
    }

    #[inline]
    fn skip_bytes(&mut self, bytes: usize) -> &str {
        let skipped_str = &self.source[self.cursor..self.cursor + bytes];
        self.cursor += bytes;
        skipped_str
    }

    /// Returns the current position of the parser considering the offset.
    #[inline]
    fn position(&self) -> TextSize {
        self.compute_position(self.cursor)
    }

    /// Computes the position of the cursor considering the offset.
    #[inline]
    fn compute_position(&self, cursor: usize) -> TextSize {
        self.offset + TextSize::try_from(cursor).unwrap()
    }

    /// Returns the next byte in the string, if there is one.
    ///
    /// # Panics
    ///
    /// When the next byte is a part of a multi-byte character.
    #[inline]
    fn next_byte(&mut self) -> Option<u8> {
        self.source[self.cursor..].as_bytes().first().map(|&byte| {
            self.cursor += 1;
            byte
        })
    }

    #[inline]
    fn next_char(&mut self) -> Option<char> {
        self.source[self.cursor..].chars().next().inspect(|c| {
            self.cursor += c.len_utf8();
        })
    }

    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.source[self.cursor..].as_bytes().first().copied()
    }

    fn parse_unicode_literal(&mut self, literal_number: usize) -> Result<char, LexicalError> {
        let mut p: u32 = 0u32;
        for i in 1..=literal_number {
            let start = self.position();
            match self.next_char() {
                Some(c) => match c.to_digit(16) {
                    Some(d) => p += d << ((literal_number - i) * 4),
                    None => {
                        return Err(LexicalError::new(
                            LexicalErrorType::UnicodeError,
                            TextRange::at(start, TextSize::try_from(c.len_utf8()).unwrap()),
                        ));
                    }
                },
                None => {
                    return Err(LexicalError::new(
                        LexicalErrorType::UnicodeError,
                        TextRange::empty(self.position()),
                    ))
                }
            }
        }
        match p {
            0xD800..=0xDFFF => Ok(std::char::REPLACEMENT_CHARACTER),
            _ => std::char::from_u32(p).ok_or(LexicalError::new(
                LexicalErrorType::UnicodeError,
                TextRange::empty(self.position()),
            )),
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

        // OK because radix_bytes is always going to be in the ASCII range.
        let radix_str = std::str::from_utf8(&radix_bytes[..len]).expect("ASCII bytes");
        let value = u32::from_str_radix(radix_str, 8).unwrap();
        char::from_u32(value).unwrap()
    }

    fn parse_unicode_name(&mut self) -> Result<char, LexicalError> {
        let start_pos = self.position();
        let Some('{') = self.next_char() else {
            return Err(LexicalError::new(
                LexicalErrorType::MissingUnicodeLbrace,
                TextRange::empty(start_pos),
            ));
        };

        let start_pos = self.position();
        let Some(close_idx) = self.source[self.cursor..].find('}') else {
            return Err(LexicalError::new(
                LexicalErrorType::MissingUnicodeRbrace,
                TextRange::empty(self.compute_position(self.source.len())),
            ));
        };

        let name_and_ending = self.skip_bytes(close_idx + 1);
        let name = &name_and_ending[..name_and_ending.len() - 1];

        unicode_names2::character(name).ok_or_else(|| {
            LexicalError::new(
                LexicalErrorType::UnicodeError,
                // The cursor is right after the `}` character, so we subtract 1 to get the correct
                // range of the unicode name.
                TextRange::new(
                    start_pos,
                    self.compute_position(self.cursor - '}'.len_utf8()),
                ),
            )
        })
    }

    /// Parse an escaped character, returning the new character.
    fn parse_escaped_char(&mut self) -> Result<Option<EscapedChar>, LexicalError> {
        let Some(first_char) = self.next_char() else {
            // TODO: check when this error case happens
            return Err(LexicalError::new(
                LexicalErrorType::StringError,
                TextRange::empty(self.position()),
            ));
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
            'u' if !self.flags.is_byte_string() => self.parse_unicode_literal(4)?,
            'U' if !self.flags.is_byte_string() => self.parse_unicode_literal(8)?,
            'N' if !self.flags.is_byte_string() => self.parse_unicode_name()?,
            // Special cases where the escape sequence is not a single character
            '\n' => return Ok(None),
            '\r' => {
                if self.peek_byte() == Some(b'\n') {
                    self.next_byte();
                }

                return Ok(None);
            }
            _ => return Ok(Some(EscapedChar::Escape(first_char))),
        };

        Ok(Some(EscapedChar::Literal(new_char)))
    }

    fn parse_fstring_middle(mut self) -> Result<ast::FStringLiteralElement, LexicalError> {
        // Fast-path: if the f-string doesn't contain any escape sequences, return the literal.
        let Some(mut index) = memchr::memchr3(b'{', b'}', b'\\', self.source.as_bytes()) else {
            return Ok(ast::FStringLiteralElement {
                value: self.source,
                range: self.range,
            });
        };

        let mut value = String::with_capacity(self.source.len());
        loop {
            // Add the characters before the escape sequence (or curly brace) to the string.
            let before_with_slash_or_brace = self.skip_bytes(index + 1);
            let before = &before_with_slash_or_brace[..before_with_slash_or_brace.len() - 1];
            value.push_str(before);

            // Add the escaped character to the string.
            match &self.source.as_bytes()[self.cursor - 1] {
                // If there are any curly braces inside a `FStringMiddle` token,
                // then they were escaped (i.e. `{{` or `}}`). This means that
                // we need increase the location by 2 instead of 1.
                b'{' => {
                    self.offset += TextSize::from(1);
                    value.push('{');
                }
                b'}' => {
                    self.offset += TextSize::from(1);
                    value.push('}');
                }
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
                b'\\' => {
                    if !self.flags.is_raw_string() && self.peek_byte().is_some() {
                        match self.parse_escaped_char()? {
                            None => {}
                            Some(EscapedChar::Literal(c)) => value.push(c),
                            Some(EscapedChar::Escape(c)) => {
                                value.push('\\');
                                value.push(c);
                            }
                        }
                    } else {
                        value.push('\\');
                    }
                }
                ch => {
                    unreachable!("Expected '{{', '}}', or '\\' but got {:?}", ch);
                }
            }

            let Some(next_index) =
                memchr::memchr3(b'{', b'}', b'\\', self.source[self.cursor..].as_bytes())
            else {
                // Add the rest of the string to the value.
                let rest = &self.source[self.cursor..];
                value.push_str(rest);
                break;
            };

            index = next_index;
        }

        Ok(ast::FStringLiteralElement {
            value: value.into_boxed_str(),
            range: self.range,
        })
    }

    fn parse_bytes(mut self) -> Result<StringType, LexicalError> {
        if let Some(index) = self.source.as_bytes().find_non_ascii_byte() {
            let ch = self.source.chars().nth(index).unwrap();
            return Err(LexicalError::new(
                LexicalErrorType::InvalidByteLiteral,
                TextRange::at(
                    self.compute_position(index),
                    TextSize::try_from(ch.len_utf8()).unwrap(),
                ),
            ));
        }

        if self.flags.is_raw_string() {
            // For raw strings, no escaping is necessary.
            return Ok(StringType::Bytes(ast::BytesLiteral {
                value: self.source.into_boxed_bytes(),
                range: self.range,
                flags: self.flags.into(),
            }));
        }

        let Some(mut escape) = memchr::memchr(b'\\', self.source.as_bytes()) else {
            // If the string doesn't contain any escape sequences, return the owned string.
            return Ok(StringType::Bytes(ast::BytesLiteral {
                value: self.source.into_boxed_bytes(),
                range: self.range,
                flags: self.flags.into(),
            }));
        };

        // If the string contains escape sequences, we need to parse them.
        let mut value = Vec::with_capacity(self.source.len());
        loop {
            // Add the characters before the escape sequence to the string.
            let before_with_slash = self.skip_bytes(escape + 1);
            let before = &before_with_slash[..before_with_slash.len() - 1];
            value.extend_from_slice(before.as_bytes());

            // Add the escaped character to the string.
            match self.parse_escaped_char()? {
                None => {}
                Some(EscapedChar::Literal(c)) => value.push(c as u8),
                Some(EscapedChar::Escape(c)) => {
                    value.push(b'\\');
                    value.push(c as u8);
                }
            }

            let Some(next_escape) = memchr::memchr(b'\\', self.source[self.cursor..].as_bytes())
            else {
                // Add the rest of the string to the value.
                let rest = &self.source[self.cursor..];
                value.extend_from_slice(rest.as_bytes());
                break;
            };

            // Update the position of the next escape sequence.
            escape = next_escape;
        }

        Ok(StringType::Bytes(ast::BytesLiteral {
            value: value.into_boxed_slice(),
            range: self.range,
            flags: self.flags.into(),
        }))
    }

    fn parse_string(mut self) -> Result<StringType, LexicalError> {
        if self.flags.is_raw_string() {
            // For raw strings, no escaping is necessary.
            return Ok(StringType::Str(ast::StringLiteral {
                value: self.source,
                range: self.range,
                flags: self.flags.into(),
            }));
        }

        let Some(mut escape) = memchr::memchr(b'\\', self.source.as_bytes()) else {
            // If the string doesn't contain any escape sequences, return the owned string.
            return Ok(StringType::Str(ast::StringLiteral {
                value: self.source,
                range: self.range,
                flags: self.flags.into(),
            }));
        };

        // If the string contains escape sequences, we need to parse them.
        let mut value = String::with_capacity(self.source.len());

        loop {
            // Add the characters before the escape sequence to the string.
            let before_with_slash = self.skip_bytes(escape + 1);
            let before = &before_with_slash[..before_with_slash.len() - 1];
            value.push_str(before);

            // Add the escaped character to the string.
            match self.parse_escaped_char()? {
                None => {}
                Some(EscapedChar::Literal(c)) => value.push(c),
                Some(EscapedChar::Escape(c)) => {
                    value.push('\\');
                    value.push(c);
                }
            }

            let Some(next_escape) = self.source[self.cursor..].find('\\') else {
                // Add the rest of the string to the value.
                let rest = &self.source[self.cursor..];
                value.push_str(rest);
                break;
            };

            // Update the position of the next escape sequence.
            escape = next_escape;
        }

        Ok(StringType::Str(ast::StringLiteral {
            value: value.into_boxed_str(),
            range: self.range,
            flags: self.flags.into(),
        }))
    }

    fn parse(self) -> Result<StringType, LexicalError> {
        if self.flags.is_byte_string() {
            self.parse_bytes()
        } else {
            self.parse_string()
        }
    }
}

pub(crate) fn parse_string_literal(
    source: Box<str>,
    flags: AnyStringFlags,
    range: TextRange,
) -> Result<StringType, LexicalError> {
    StringParser::new(source, flags, range.start() + flags.opener_len(), range).parse()
}

// TODO(dhruvmanila): Move this to the new parser
pub(crate) fn parse_fstring_literal_element(
    source: Box<str>,
    flags: AnyStringFlags,
    range: TextRange,
) -> Result<ast::FStringLiteralElement, LexicalError> {
    StringParser::new(source, flags, range.start(), range).parse_fstring_middle()
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::Suite;

    use crate::error::LexicalErrorType;
    use crate::{parse_module, FStringErrorType, ParseError, ParseErrorType, Parsed};

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    fn parse_suite(source: &str) -> Result<Suite, ParseError> {
        parse_module(source).map(Parsed::into_suite)
    }

    fn string_parser_escaped_eol(eol: &str) -> Suite {
        let source = format!(r"'text \{eol}more text'");
        parse_suite(&source).unwrap()
    }

    #[test]
    fn test_string_parser_escaped_unix_eol() {
        let suite = string_parser_escaped_eol(UNIX_EOL);
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_string_parser_escaped_mac_eol() {
        let suite = string_parser_escaped_eol(MAC_EOL);
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_string_parser_escaped_windows_eol() {
        let suite = string_parser_escaped_eol(WINDOWS_EOL);
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring() {
        let source = r#"f"{a}{ b }{{foo}}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_nested_spec() {
        let source = r#"f"{foo:{spec}}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_not_nested_spec() {
        let source = r#"f"{foo:spec}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_empty_fstring() {
        let source = r#"f"""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_parse_self_documenting_base() {
        let source = r#"f"{user=}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_parse_self_documenting_base_more() {
        let source = r#"f"mix {user=} with text and {second=}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_parse_self_documenting_format() {
        let source = r#"f"{user=:>10}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    fn parse_fstring_error(source: &str) -> FStringErrorType {
        parse_suite(source)
            .map_err(|e| match e.error {
                ParseErrorType::Lexical(LexicalErrorType::FStringError(e)) => e,
                ParseErrorType::FStringError(e) => e,
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
        // NOTE: The parser produces the `LambdaWithoutParentheses` for this case, but
        // since the parser only return the first error to maintain compatibility with
        // the rest of the codebase, this test case fails. The `LambdaWithoutParentheses`
        // error appears after the unexpected `FStringMiddle` token, which is between the
        // `:` and the `{`.
        // assert_eq!(parse_fstring_error("f'{lambda x: {x}}'"), LambdaWithoutParentheses);
        assert!(parse_suite(r#"f"{class}""#).is_err());
    }

    #[test]
    fn test_parse_fstring_not_equals() {
        let source = r#"f"{1 != 2}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_equals() {
        let source = r#"f"{42 == 42}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_self_doc_prec_space() {
        let source = r#"f"{x   =}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_self_doc_trailing_space() {
        let source = r#"f"{x=   }""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_yield_expr() {
        let source = r#"f"{yield}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_string_concat() {
        let source = "'Hello ' 'world'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_u_string_concat_1() {
        let source = "'Hello ' u'world'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_u_string_concat_2() {
        let source = "u'Hello ' 'world'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_f_string_concat_1() {
        let source = "'Hello ' f'world'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_f_string_concat_2() {
        let source = "'Hello ' f'world'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_f_string_concat_3() {
        let source = "'Hello ' f'world{\"!\"}'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_f_string_concat_4() {
        let source = "'Hello ' f'world{\"!\"}' 'again!'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_u_f_string_concat_1() {
        let source = "u'Hello ' f'world'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_u_f_string_concat_2() {
        let source = "u'Hello ' f'world' '!'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_string_triple_quotes_with_kind() {
        let source = "u'''Hello, world!'''";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_single_quoted_byte() {
        // single quote
        let source = r##"b'\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff'"##;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_double_quoted_byte() {
        // double quote
        let source = r##"b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff""##;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_escape_char_in_byte_literal() {
        // backslash does not escape
        let source = r#"b"omkmok\Xaa""#; // spell-checker:ignore omkmok
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_raw_byte_literal_1() {
        let source = r"rb'\x1z'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_raw_byte_literal_2() {
        let source = r"rb'\\'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_escape_octet() {
        let source = r"b'\43a\4\1234'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_escaped_newline() {
        let source = r#"f"\n{x}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_constant_range() {
        let source = r#"f"aaa{bbb}ccc{ddd}eee""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_unescaped_newline() {
        let source = r#"f"""
{x}""""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_escaped_character() {
        let source = r#"f"\\{x}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_raw_fstring() {
        let source = r#"rf"{x}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_triple_quoted_raw_fstring() {
        let source = r#"rf"""{x}""""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_fstring_line_continuation() {
        let source = r#"rf"\
{x}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_nested_string_spec() {
        let source = r#"f"{foo:{''}}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_parse_fstring_nested_concatenation_string_spec() {
        let source = r#"f"{foo:{'' ''}}""#;
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    /// <https://github.com/astral-sh/ruff/issues/8355>
    #[test]
    fn test_dont_panic_on_8_in_octal_escape() {
        let source = r"bold = '\038[1m'";
        let suite = parse_suite(source).unwrap();
        insta::assert_debug_snapshot!(suite);
    }

    #[test]
    fn test_invalid_unicode_literal() {
        let source = r"'\x1ó34'";
        let error = parse_suite(source).unwrap_err();
        insta::assert_debug_snapshot!(error);
    }

    #[test]
    fn test_missing_unicode_lbrace_error() {
        let source = r"'\N '";
        let error = parse_suite(source).unwrap_err();
        insta::assert_debug_snapshot!(error);
    }

    #[test]
    fn test_missing_unicode_rbrace_error() {
        let source = r"'\N{SPACE'";
        let error = parse_suite(source).unwrap_err();
        insta::assert_debug_snapshot!(error);
    }

    #[test]
    fn test_invalid_unicode_name_error() {
        let source = r"'\N{INVALID}'";
        let error = parse_suite(source).unwrap_err();
        insta::assert_debug_snapshot!(error);
    }

    #[test]
    fn test_invalid_byte_literal_error() {
        let source = r"b'123a𝐁c'";
        let error = parse_suite(source).unwrap_err();
        insta::assert_debug_snapshot!(error);
    }

    macro_rules! test_aliases_parse {
        ($($name:ident: $alias:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let source = format!(r#""\N{{{0}}}""#, $alias);
                let suite = parse_suite(&source).unwrap();
                insta::assert_debug_snapshot!(suite);
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
