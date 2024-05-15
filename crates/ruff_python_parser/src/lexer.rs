//! This module takes care of lexing Python source text.
//!
//! This means source code is scanned and translated into separate tokens. The rules
//! governing what is and is not a valid token are defined in the Python reference
//! guide section on [Lexical analysis].
//!
//! The primary function in this module is [`lex`], which takes a string slice
//! and returns an iterator over the tokens in the source code. The tokens are currently returned
//! as a `Result<Spanned, LexicalError>`, where [`Spanned`] is a tuple containing the
//! start and end [`TextSize`] and a [`Tok`] denoting the token.
//!
//! # Example
//!
//! ```
//! use ruff_python_parser::{lexer::lex, Tok, Mode};
//!
//! let source = "x = 'RustPython'";
//! let tokens = lex(source, Mode::Module)
//!     .map(|tok| tok.expect("Failed to lex"))
//!     .collect::<Vec<_>>();
//!
//! for (token, range) in tokens {
//!     println!(
//!         "{token:?}@{range:?}",
//!     );
//! }
//! ```
//!
//! [Lexical analysis]: https://docs.python.org/3/reference/lexical_analysis.html

use std::{char, cmp::Ordering, str::FromStr};

use unicode_ident::{is_xid_continue, is_xid_start};
use unicode_normalization::UnicodeNormalization;

use ruff_python_ast::{
    str::Quote, AnyStringFlags, AnyStringPrefix, FStringPrefix, Int, IpyEscapeKind,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::error::FStringErrorType;
use crate::lexer::cursor::{Cursor, EOF_CHAR};
use crate::lexer::fstring::{FStringContext, FStrings, FStringsCheckpoint};
use crate::lexer::indentation::{Indentation, Indentations, IndentationsCheckpoint};
use crate::{Mode, TokenKind};

mod cursor;
mod fstring;
mod indentation;

/// A lexer for Python source code.
#[derive(Debug)]
pub struct Lexer<'src> {
    /// Source code to be lexed.
    source: &'src str,

    /// A pointer to the current character of the source code which is being lexed.
    cursor: Cursor<'src>,

    /// The current lexed token.
    current: Token,

    /// The value of the current token.
    value: TokenValue,

    /// Lexer state.
    state: State,

    /// Represents the current level of nesting in the lexer, indicating the depth of parentheses.
    /// The lexer is within a parenthesized context if the value is greater than 0.
    nesting: u32,

    /// A stack of indentation representing the current indentation level.
    indentations: Indentations,
    pending_indentation: Option<Indentation>,

    /// Lexer mode.
    mode: Mode,

    /// F-string contexts.
    fstrings: FStrings,

    /// Errors encountered while lexing.
    errors: Vec<LexicalError>,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given input source which starts at the given offset.
    ///
    /// If the start offset is greater than 0, the cursor is moved ahead that many bytes.
    /// This means that the input source should be the complete source code and not the
    /// sliced version.
    pub(crate) fn new(source: &'src str, mode: Mode, start_offset: TextSize) -> Self {
        assert!(
            u32::try_from(source.len()).is_ok(),
            "Lexer only supports files with a size up to 4GB"
        );

        let mut lexer = Lexer {
            source,
            cursor: Cursor::new(source),
            state: State::AfterNewline,
            value: TokenValue::None,
            current: Token::new(TokenKind::EndOfFile, TextRange::empty(start_offset)),
            nesting: 0,
            indentations: Indentations::default(),
            pending_indentation: None,
            mode,
            fstrings: FStrings::default(),
            errors: Vec::new(),
        };

        // TODO: Handle possible mismatch between BOM and explicit encoding declaration.
        // spell-checker:ignore feff
        lexer.cursor.eat_char('\u{feff}');

        if start_offset > TextSize::new(0) {
            lexer.cursor.skip_bytes(start_offset.to_usize());
        }

        lexer
    }

    /// Returns the kind of the current token.
    pub(crate) fn current_kind(&self) -> TokenKind {
        self.current.kind()
    }

    /// Returns the range of the current token.
    pub(crate) fn current_range(&self) -> TextRange {
        self.current.range()
    }

    /// Helper function to push the given error and return the [`TokenKind::Unknown`] token.
    fn push_error(&mut self, error: LexicalError) -> TokenKind {
        self.errors.push(error);
        TokenKind::Unknown
    }

    /// Helper function to push the given error and return the [`Token`] with [`TokenKind::Unknown`]
    /// kind and the error location.
    fn push_error_with_range(&mut self, error: LexicalError) -> Token {
        let range = error.location();
        self.errors.push(error);
        Token::new(TokenKind::Unknown, range)
    }

    /// Lex an identifier. Also used for keywords and string/bytes literals with a prefix.
    fn lex_identifier(&mut self, first: char) -> TokenKind {
        // Detect potential string like rb'' b'' f'' u'' r''
        match (first, self.cursor.first()) {
            ('f' | 'F', quote @ ('\'' | '"')) => {
                self.cursor.bump();
                return self.lex_fstring_start(quote, FStringPrefix::Regular);
            }
            ('r', 'f' | 'F') | ('f' | 'F', 'r') if is_quote(self.cursor.second()) => {
                self.cursor.bump();
                let quote = self.cursor.bump().unwrap();
                return self.lex_fstring_start(quote, FStringPrefix::Raw { uppercase_r: false });
            }
            ('R', 'f' | 'F') | ('f' | 'F', 'R') if is_quote(self.cursor.second()) => {
                self.cursor.bump();
                let quote = self.cursor.bump().unwrap();
                return self.lex_fstring_start(quote, FStringPrefix::Raw { uppercase_r: true });
            }
            (_, quote @ ('\'' | '"')) => {
                if let Ok(prefix) = AnyStringPrefix::try_from(first) {
                    self.cursor.bump();
                    return self.lex_string(prefix, quote);
                }
            }
            (_, second @ ('r' | 'R' | 'b' | 'B')) if is_quote(self.cursor.second()) => {
                self.cursor.bump();
                if let Ok(prefix) = AnyStringPrefix::try_from([first, second]) {
                    let quote = self.cursor.bump().unwrap();
                    return self.lex_string(prefix, quote);
                }
            }
            _ => {}
        }

        // Keep track of whether the identifier is ASCII-only or not.
        //
        // This is important because Python applies NFKC normalization to
        // identifiers: https://docs.python.org/3/reference/lexical_analysis.html#identifiers.
        // We need to therefore do the same in our lexer, but applying NFKC normalization
        // unconditionally is extremely expensive. If we know an identifier is ASCII-only,
        // (by far the most common case), we can skip NFKC normalization of the identifier.
        let mut is_ascii = first.is_ascii();
        self.cursor
            .eat_while(|c| is_identifier_continuation(c, &mut is_ascii));

        let text = self.token_text();

        if !is_ascii {
            self.value = TokenValue::Name(text.nfkc().collect::<String>().into_boxed_str());
            return TokenKind::Name;
        }

        match text {
            "False" => TokenKind::False,
            "None" => TokenKind::None,
            "True" => TokenKind::True,
            "and" => TokenKind::And,
            "as" => TokenKind::As,
            "assert" => TokenKind::Assert,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "break" => TokenKind::Break,
            "case" => TokenKind::Case,
            "class" => TokenKind::Class,
            "continue" => TokenKind::Continue,
            "def" => TokenKind::Def,
            "del" => TokenKind::Del,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "except" => TokenKind::Except,
            "finally" => TokenKind::Finally,
            "for" => TokenKind::For,
            "from" => TokenKind::From,
            "global" => TokenKind::Global,
            "if" => TokenKind::If,
            "import" => TokenKind::Import,
            "in" => TokenKind::In,
            "is" => TokenKind::Is,
            "lambda" => TokenKind::Lambda,
            "match" => TokenKind::Match,
            "nonlocal" => TokenKind::Nonlocal,
            "not" => TokenKind::Not,
            "or" => TokenKind::Or,
            "pass" => TokenKind::Pass,
            "raise" => TokenKind::Raise,
            "return" => TokenKind::Return,
            "try" => TokenKind::Try,
            "type" => TokenKind::Type,
            "while" => TokenKind::While,
            "with" => TokenKind::With,
            "yield" => TokenKind::Yield,
            _ => {
                self.value = TokenValue::Name(text.to_string().into_boxed_str());
                TokenKind::Name
            }
        }
    }

    /// Numeric lexing. The feast can start!
    fn lex_number(&mut self, first: char) -> TokenKind {
        if first == '0' {
            if self.cursor.eat_if(|c| matches!(c, 'x' | 'X')).is_some() {
                self.lex_number_radix(Radix::Hex)
            } else if self.cursor.eat_if(|c| matches!(c, 'o' | 'O')).is_some() {
                self.lex_number_radix(Radix::Octal)
            } else if self.cursor.eat_if(|c| matches!(c, 'b' | 'B')).is_some() {
                self.lex_number_radix(Radix::Binary)
            } else {
                self.lex_decimal_number(first)
            }
        } else {
            self.lex_decimal_number(first)
        }
    }

    /// Lex a hex/octal/decimal/binary number without a decimal point.
    fn lex_number_radix(&mut self, radix: Radix) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert!(matches!(
            self.cursor.previous().to_ascii_lowercase(),
            'x' | 'o' | 'b'
        ));

        // Lex the portion of the token after the base prefix (e.g., `9D5` in `0x9D5`).
        let mut number = LexedText::new(self.offset(), self.source);
        self.radix_run(&mut number, radix);

        // Extract the entire number, including the base prefix (e.g., `0x9D5`).
        let token = &self.source[self.token_range()];

        let value = match Int::from_str_radix(number.as_str(), radix.as_u32(), token) {
            Ok(int) => int,
            Err(err) => {
                return self.push_error(LexicalError::new(
                    LexicalErrorType::OtherError(format!("{err:?}").into_boxed_str()),
                    self.token_range(),
                ));
            }
        };
        self.value = TokenValue::Int(value);
        TokenKind::Int
    }

    /// Lex a normal number, that is, no octal, hex or binary number.
    fn lex_decimal_number(&mut self, first_digit_or_dot: char) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert!(self.cursor.previous().is_ascii_digit() || self.cursor.previous() == '.');
        let start_is_zero = first_digit_or_dot == '0';

        let mut number = LexedText::new(self.token_start(), self.source);
        if first_digit_or_dot != '.' {
            number.push(first_digit_or_dot);
            self.radix_run(&mut number, Radix::Decimal);
        };

        let is_float = if first_digit_or_dot == '.' || self.cursor.eat_char('.') {
            number.push('.');

            if self.cursor.eat_char('_') {
                return self.push_error(LexicalError::new(
                    LexicalErrorType::OtherError("Invalid Syntax".to_string().into_boxed_str()),
                    TextRange::new(self.offset() - TextSize::new(1), self.offset()),
                ));
            }

            self.radix_run(&mut number, Radix::Decimal);
            true
        } else {
            // Normal number:
            false
        };

        let is_float = match self.cursor.rest().as_bytes() {
            [b'e' | b'E', b'0'..=b'9', ..] | [b'e' | b'E', b'-' | b'+', b'0'..=b'9', ..] => {
                // 'e' | 'E'
                number.push(self.cursor.bump().unwrap());

                if let Some(sign) = self.cursor.eat_if(|c| matches!(c, '+' | '-')) {
                    number.push(sign);
                }

                self.radix_run(&mut number, Radix::Decimal);

                true
            }
            _ => is_float,
        };

        if is_float {
            // Improvement: Use `Cow` instead of pushing to value text
            let Ok(value) = f64::from_str(number.as_str()) else {
                return self.push_error(LexicalError::new(
                    LexicalErrorType::OtherError(
                        "Invalid decimal literal".to_string().into_boxed_str(),
                    ),
                    self.token_range(),
                ));
            };

            // Parse trailing 'j':
            if self.cursor.eat_if(|c| matches!(c, 'j' | 'J')).is_some() {
                self.value = TokenValue::Complex {
                    real: 0.0,
                    imag: value,
                };
                TokenKind::Complex
            } else {
                self.value = TokenValue::Float(value);
                TokenKind::Float
            }
        } else {
            // Parse trailing 'j':
            if self.cursor.eat_if(|c| matches!(c, 'j' | 'J')).is_some() {
                let imag = f64::from_str(number.as_str()).unwrap();
                self.value = TokenValue::Complex { real: 0.0, imag };
                TokenKind::Complex
            } else {
                let value = match Int::from_str(number.as_str()) {
                    Ok(value) => {
                        if start_is_zero && value.as_u8() != Some(0) {
                            // Leading zeros in decimal integer literals are not permitted.
                            return self.push_error(LexicalError::new(
                                LexicalErrorType::OtherError(
                                    "Invalid decimal integer literal"
                                        .to_string()
                                        .into_boxed_str(),
                                ),
                                self.token_range(),
                            ));
                        }
                        value
                    }
                    Err(err) => {
                        return self.push_error(LexicalError::new(
                            LexicalErrorType::OtherError(format!("{err:?}").into_boxed_str()),
                            self.token_range(),
                        ))
                    }
                };
                self.value = TokenValue::Int(value);
                TokenKind::Int
            }
        }
    }

    /// Consume a sequence of numbers with the given radix,
    /// the digits can be decorated with underscores
    /// like this: '`1_2_3_4`' == '1234'
    fn radix_run(&mut self, number: &mut LexedText, radix: Radix) {
        loop {
            if let Some(c) = self.cursor.eat_if(|c| radix.is_digit(c)) {
                number.push(c);
            }
            // Number that contains `_` separators. Remove them from the parsed text.
            else if self.cursor.first() == '_' && radix.is_digit(self.cursor.second()) {
                // Skip over `_`
                self.cursor.bump();
                number.skip_char();
            } else {
                break;
            }
        }
    }

    /// Lex a single comment.
    fn lex_comment(&mut self) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), '#');

        let bytes = self.cursor.rest().as_bytes();
        let offset = memchr::memchr2(b'\n', b'\r', bytes).unwrap_or(bytes.len());
        self.cursor.skip_bytes(offset);

        TokenKind::Comment
    }

    /// Lex a single `IPython` escape command.
    fn lex_ipython_escape_command(&mut self, escape_kind: IpyEscapeKind) -> TokenKind {
        let mut value = String::new();

        loop {
            match self.cursor.first() {
                '\\' => {
                    // Only skip the line continuation if it is followed by a newline
                    // otherwise it is a normal backslash which is part of the magic command:
                    //
                    //        Skip this backslash
                    //        v
                    //   !pwd \
                    //      && ls -a | sed 's/^/\\    /'
                    //                          ^^
                    //                          Don't skip these backslashes
                    if self.cursor.second() == '\r' {
                        self.cursor.bump();
                        self.cursor.bump();
                        self.cursor.eat_char('\n');
                        continue;
                    } else if self.cursor.second() == '\n' {
                        self.cursor.bump();
                        self.cursor.bump();
                        continue;
                    }

                    self.cursor.bump();
                    value.push('\\');
                }
                // Help end escape commands are those that end with 1 or 2 question marks.
                // Here, we're only looking for a subset of help end escape commands which
                // are the ones that has the escape token at the start of the line as well.
                // On the other hand, we're not looking for help end escape commands that
                // are strict in the sense that the escape token is only at the end. For example,
                //
                //   * `%foo?` is recognized as a help end escape command but not as a strict one.
                //   * `foo?` is recognized as a strict help end escape command which is not
                //     lexed here but is identified at the parser level.
                //
                // Help end escape commands implemented in the IPython codebase using regex:
                // https://github.com/ipython/ipython/blob/292e3a23459ca965b8c1bfe2c3707044c510209a/IPython/core/inputtransformer2.py#L454-L462
                '?' => {
                    self.cursor.bump();
                    let mut question_count = 1u32;
                    while self.cursor.eat_char('?') {
                        question_count += 1;
                    }

                    // The original implementation in the IPython codebase is based on regex which
                    // means that it's strict in the sense that it won't recognize a help end escape:
                    //   * If there's any whitespace before the escape token (e.g. `%foo ?`)
                    //   * If there are more than 2 question mark tokens (e.g. `%foo???`)
                    // which is what we're doing here as well. In that case, we'll continue with
                    // the prefixed escape token.
                    //
                    // Now, the whitespace and empty value check also makes sure that an empty
                    // command (e.g. `%?` or `? ??`, no value after/between the escape tokens)
                    // is not recognized as a help end escape command. So, `%?` and `? ??` are
                    // `IpyEscapeKind::Magic` and `IpyEscapeKind::Help` because of the initial `%` and `??`
                    // tokens.
                    if question_count > 2
                        || value.chars().last().map_or(true, is_python_whitespace)
                        || !matches!(self.cursor.first(), '\n' | '\r' | EOF_CHAR)
                    {
                        // Not a help end escape command, so continue with the lexing.
                        value.reserve(question_count as usize);
                        for _ in 0..question_count {
                            value.push('?');
                        }
                        continue;
                    }

                    if escape_kind.is_help() {
                        // If we've recognize this as a help end escape command, then
                        // any question mark token / whitespaces at the start are not
                        // considered as part of the value.
                        //
                        // For example, `??foo?` is recognized as `IpyEscapeKind::Help` and
                        // `value` is `foo` instead of `??foo`.
                        value = value.trim_start_matches([' ', '?']).to_string();
                    } else if escape_kind.is_magic() {
                        // Between `%` and `?` (at the end), the `?` takes priority
                        // over the `%` so `%foo?` is recognized as `IpyEscapeKind::Help`
                        // and `value` is `%foo` instead of `foo`. So, we need to
                        // insert the magic escape token at the start.
                        value.insert_str(0, escape_kind.as_str());
                    }

                    let kind = match question_count {
                        1 => IpyEscapeKind::Help,
                        2 => IpyEscapeKind::Help2,
                        _ => unreachable!("`question_count` is always 1 or 2"),
                    };

                    self.value = TokenValue::IpyEscapeCommand {
                        kind,
                        value: value.into_boxed_str(),
                    };

                    return TokenKind::IpyEscapeCommand;
                }
                '\n' | '\r' | EOF_CHAR => {
                    self.value = TokenValue::IpyEscapeCommand {
                        kind: escape_kind,
                        value: value.into_boxed_str(),
                    };

                    return TokenKind::IpyEscapeCommand;
                }
                c => {
                    self.cursor.bump();
                    value.push(c);
                }
            }
        }
    }

    /// Lex a f-string start token.
    fn lex_fstring_start(&mut self, quote: char, prefix: FStringPrefix) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), quote);

        let mut flags = AnyStringFlags::default()
            .with_prefix(AnyStringPrefix::Format(prefix))
            .with_quote_style(if quote == '"' {
                Quote::Double
            } else {
                Quote::Single
            });

        if self.cursor.eat_char2(quote, quote) {
            flags = flags.with_triple_quotes();
        }

        self.fstrings.push(FStringContext::new(flags, self.nesting));

        self.value = TokenValue::FStringStart(flags);
        TokenKind::FStringStart
    }

    /// Lex a f-string middle or end token.
    fn lex_fstring_middle_or_end(&mut self) -> Option<TokenKind> {
        // SAFETY: Safe because the function is only called when `self.fstrings` is not empty.
        let fstring = self.fstrings.current().unwrap();
        self.cursor.start_token();

        // Check if we're at the end of the f-string.
        if fstring.is_triple_quoted() {
            let quote_char = fstring.quote_char();
            if self.cursor.eat_char3(quote_char, quote_char, quote_char) {
                return Some(TokenKind::FStringEnd);
            }
        } else if self.cursor.eat_char(fstring.quote_char()) {
            return Some(TokenKind::FStringEnd);
        }

        // We have to decode `{{` and `}}` into `{` and `}` respectively. As an
        // optimization, we only allocate a new string we find any escaped curly braces,
        // otherwise this string will remain empty and we'll use a source slice instead.
        let mut normalized = String::new();

        // Tracks the last offset of token value that has been written to `normalized`.
        let mut last_offset = self.offset();

        // This isn't going to change for the duration of the loop.
        let in_format_spec = fstring.is_in_format_spec(self.nesting);

        let mut in_named_unicode = false;

        loop {
            match self.cursor.first() {
                // The condition is to differentiate between the `NUL` (`\0`) character
                // in the source code and the one returned by `self.cursor.first()` when
                // we reach the end of the source code.
                EOF_CHAR if self.cursor.is_eof() => {
                    let error = if fstring.is_triple_quoted() {
                        FStringErrorType::UnterminatedTripleQuotedString
                    } else {
                        FStringErrorType::UnterminatedString
                    };
                    self.fstrings.pop();
                    return Some(self.push_error(LexicalError::new(
                        LexicalErrorType::FStringError(error),
                        self.token_range(),
                    )));
                }
                '\n' | '\r' if !fstring.is_triple_quoted() => {
                    // If we encounter a newline while we're in a format spec, then
                    // we stop here and let the lexer emit the newline token.
                    //
                    // Relevant discussion: https://github.com/python/cpython/issues/110259
                    if in_format_spec {
                        break;
                    }
                    self.fstrings.pop();
                    return Some(self.push_error(LexicalError::new(
                        LexicalErrorType::FStringError(FStringErrorType::UnterminatedString),
                        self.token_range(),
                    )));
                }
                '\\' => {
                    self.cursor.bump(); // '\'
                    if matches!(self.cursor.first(), '{' | '}') {
                        // Don't consume `{` or `}` as we want them to be emitted as tokens.
                        // They will be handled in the next iteration.
                        continue;
                    } else if !fstring.is_raw_string() {
                        if self.cursor.eat_char2('N', '{') {
                            in_named_unicode = true;
                            continue;
                        }
                    }
                    // Consume the escaped character.
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else {
                        self.cursor.bump();
                    }
                }
                quote @ ('\'' | '"') if quote == fstring.quote_char() => {
                    if let Some(triple_quotes) = fstring.triple_quotes() {
                        if self.cursor.rest().starts_with(triple_quotes) {
                            break;
                        }
                        self.cursor.bump();
                    } else {
                        break;
                    }
                }
                '{' => {
                    if self.cursor.second() == '{' && !in_format_spec {
                        self.cursor.bump();
                        normalized
                            .push_str(&self.source[TextRange::new(last_offset, self.offset())]);
                        self.cursor.bump(); // Skip the second `{`
                        last_offset = self.offset();
                    } else {
                        break;
                    }
                }
                '}' => {
                    if in_named_unicode {
                        in_named_unicode = false;
                        self.cursor.bump();
                    } else if self.cursor.second() == '}' && !in_format_spec {
                        self.cursor.bump();
                        normalized
                            .push_str(&self.source[TextRange::new(last_offset, self.offset())]);
                        self.cursor.bump(); // Skip the second `}`
                        last_offset = self.offset();
                    } else {
                        break;
                    }
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }
        let range = self.token_range();
        if range.is_empty() {
            return None;
        }

        let value = if normalized.is_empty() {
            self.source[range].to_string()
        } else {
            normalized.push_str(&self.source[TextRange::new(last_offset, self.offset())]);
            normalized
        };

        self.value = TokenValue::FStringMiddle {
            value: value.into_boxed_str(),
            flags: fstring.flags(),
        };

        Some(TokenKind::FStringMiddle)
    }

    /// Lex a string literal.
    fn lex_string(&mut self, prefix: AnyStringPrefix, quote: char) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), quote);

        let mut flags = AnyStringFlags::default()
            .with_prefix(prefix)
            .with_quote_style(if quote == '"' {
                Quote::Double
            } else {
                Quote::Single
            });

        // If the next two characters are also the quote character, then we have a triple-quoted
        // string; consume those two characters and ensure that we require a triple-quote to close
        if self.cursor.eat_char2(quote, quote) {
            flags = flags.with_triple_quotes();
        }

        let value_start = self.offset();

        let quote_byte = u8::try_from(quote).expect("char that fits in u8");
        let value_end = if flags.is_triple_quoted() {
            // For triple-quoted strings, scan until we find the closing quote (ignoring escaped
            // quotes) or the end of the file.
            loop {
                let Some(index) = memchr::memchr(quote_byte, self.cursor.rest().as_bytes()) else {
                    self.cursor.skip_to_end();

                    return self.push_error(LexicalError::new(
                        LexicalErrorType::UnclosedStringError,
                        self.token_range(),
                    ));
                };

                // Rare case: if there are an odd number of backslashes before the quote, then
                // the quote is escaped and we should continue scanning.
                let num_backslashes = self.cursor.rest().as_bytes()[..index]
                    .iter()
                    .rev()
                    .take_while(|&&c| c == b'\\')
                    .count();

                // Advance the cursor past the quote and continue scanning.
                self.cursor.skip_bytes(index + 1);

                // If the character is escaped, continue scanning.
                if num_backslashes % 2 == 1 {
                    continue;
                }

                // Otherwise, if it's followed by two more quotes, then we're done.
                if self.cursor.eat_char2(quote, quote) {
                    break self.offset() - TextSize::new(3);
                }
            }
        } else {
            // For non-triple-quoted strings, scan until we find the closing quote, but end early
            // if we encounter a newline or the end of the file.
            loop {
                let Some(index) =
                    memchr::memchr3(quote_byte, b'\r', b'\n', self.cursor.rest().as_bytes())
                else {
                    self.cursor.skip_to_end();

                    return self.push_error(LexicalError::new(
                        LexicalErrorType::StringError,
                        self.token_range(),
                    ));
                };

                // Rare case: if there are an odd number of backslashes before the quote, then
                // the quote is escaped and we should continue scanning.
                let num_backslashes = self.cursor.rest().as_bytes()[..index]
                    .iter()
                    .rev()
                    .take_while(|&&c| c == b'\\')
                    .count();

                // Skip up to the current character.
                self.cursor.skip_bytes(index);
                let ch = self.cursor.bump();

                // If the character is escaped, continue scanning.
                if num_backslashes % 2 == 1 {
                    if ch == Some('\r') {
                        self.cursor.eat_char('\n');
                    }
                    continue;
                }

                match ch {
                    Some('\r' | '\n') => {
                        return self.push_error(LexicalError::new(
                            LexicalErrorType::UnclosedStringError,
                            self.token_range(),
                        ));
                    }
                    Some(ch) if ch == quote => {
                        break self.offset() - TextSize::new(1);
                    }
                    _ => unreachable!("memchr2 returned an index that is not a quote or a newline"),
                }
            }
        };

        self.value = TokenValue::String {
            value: self.source[TextRange::new(value_start, value_end)]
                .to_string()
                .into_boxed_str(),
            flags,
        };

        TokenKind::String
    }

    pub fn next_token(&mut self) -> Token {
        self.current = self.next_token_impl();
        self.current
    }

    fn next_token_impl(&mut self) -> Token {
        if let Some(fstring) = self.fstrings.current() {
            if !fstring.is_in_expression(self.nesting) {
                if let Some(token) = self.lex_fstring_middle_or_end() {
                    if matches!(token, TokenKind::FStringEnd) {
                        self.fstrings.pop();
                    }
                    return Token::new(token, self.token_range());
                }
            }
        }
        // Return dedent tokens until the current indentation level matches the indentation of the next token.
        else if let Some(indentation) = self.pending_indentation.take() {
            match self.indentations.current().try_compare(indentation) {
                Ok(Ordering::Greater) => {
                    self.pending_indentation = Some(indentation);
                    if self.indentations.dedent_one(indentation).is_err() {
                        return self.push_error_with_range(LexicalError::new(
                            LexicalErrorType::IndentationError,
                            self.token_range(),
                        ));
                    }
                    return Token::new(TokenKind::Dedent, TextRange::empty(self.offset()));
                }
                Ok(_) => {}
                Err(_) => {
                    return self.push_error_with_range(LexicalError::new(
                        LexicalErrorType::IndentationError,
                        self.token_range(),
                    ));
                }
            }
        }

        if self.state.is_after_newline() {
            if let Some(indentation) = self.eat_indentation() {
                return indentation;
            }
        } else {
            if let Err(error) = self.skip_whitespace() {
                return self.push_error_with_range(error);
            }
        }

        self.cursor.start_token();
        if let Some(c) = self.cursor.bump() {
            if c.is_ascii() {
                self.consume_ascii_character(c)
            } else if is_unicode_identifier_start(c) {
                let identifier = self.lex_identifier(c);
                self.state = State::Other;

                Token::new(identifier, self.token_range())
            } else {
                self.push_error_with_range(LexicalError::new(
                    LexicalErrorType::UnrecognizedToken { tok: c },
                    self.token_range(),
                ))
            }
        } else {
            // Reached the end of the file. Emit a trailing newline token if not at the beginning of a logical line,
            // empty the dedent stack, and finally, return the EndOfFile token.
            self.consume_end()
        }
    }

    fn skip_whitespace(&mut self) -> Result<(), LexicalError> {
        loop {
            match self.cursor.first() {
                ' ' => {
                    self.cursor.bump();
                }
                '\t' => {
                    self.cursor.bump();
                }
                '\\' => {
                    self.cursor.bump();
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else if self.cursor.is_eof() {
                        return Err(LexicalError::new(LexicalErrorType::Eof, self.token_range()));
                    } else if !self.cursor.eat_char('\n') {
                        return Err(LexicalError::new(
                            LexicalErrorType::LineContinuationError,
                            self.token_range(),
                        ));
                    }
                }
                // Form feed
                '\x0C' => {
                    self.cursor.bump();
                }
                _ => break,
            }
        }

        Ok(())
    }

    fn eat_indentation(&mut self) -> Option<Token> {
        let mut indentation = Indentation::root();
        self.cursor.start_token();

        loop {
            match self.cursor.first() {
                ' ' => {
                    self.cursor.bump();
                    indentation = indentation.add_space();
                }
                '\t' => {
                    self.cursor.bump();
                    indentation = indentation.add_tab();
                }
                '\\' => {
                    self.cursor.bump();
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else if self.cursor.is_eof() {
                        return Some(self.push_error_with_range(LexicalError::new(
                            LexicalErrorType::Eof,
                            self.token_range(),
                        )));
                    } else if !self.cursor.eat_char('\n') {
                        return Some(self.push_error_with_range(LexicalError::new(
                            LexicalErrorType::LineContinuationError,
                            self.token_range(),
                        )));
                    }
                    indentation = Indentation::root();
                }
                // Form feed
                '\x0C' => {
                    self.cursor.bump();
                    indentation = Indentation::root();
                }
                _ => break,
            }
        }

        // Handle indentation if this is a new, not all empty, logical line
        if !matches!(self.cursor.first(), '\n' | '\r' | '#' | EOF_CHAR) {
            self.state = State::NonEmptyLogicalLine;

            // Set to false so that we don't handle indentation on the next call.
            return self.handle_indentation(indentation);
        }

        None
    }

    fn handle_indentation(&mut self, indentation: Indentation) -> Option<Token> {
        let token = match self.indentations.current().try_compare(indentation) {
            // Dedent
            Ok(Ordering::Greater) => {
                self.pending_indentation = Some(indentation);

                if self.indentations.dedent_one(indentation).is_err() {
                    return Some(self.push_error_with_range(LexicalError::new(
                        LexicalErrorType::IndentationError,
                        self.token_range(),
                    )));
                };

                Some(Token::new(
                    TokenKind::Dedent,
                    TextRange::empty(self.offset()),
                ))
            }

            Ok(Ordering::Equal) => None,

            // Indent
            Ok(Ordering::Less) => {
                self.indentations.indent(indentation);
                Some(Token::new(TokenKind::Indent, self.token_range()))
            }
            Err(_) => {
                return Some(self.push_error_with_range(LexicalError::new(
                    LexicalErrorType::IndentationError,
                    self.token_range(),
                )));
            }
        };

        token
    }

    fn consume_end(&mut self) -> Token {
        // We reached end of file.
        // First of all, we need all nestings to be finished.
        if self.nesting > 0 {
            // Reset the nesting to avoid going into infinite loop.
            self.nesting = 0;
            return self.push_error_with_range(LexicalError::new(
                LexicalErrorType::Eof,
                self.token_range(),
            ));
        }

        // Next, insert a trailing newline, if required.
        if !self.state.is_new_logical_line() {
            self.state = State::AfterNewline;
            Token::new(TokenKind::Newline, TextRange::empty(self.offset()))
        }
        // Next, flush the indentation stack to zero.
        else if self.indentations.dedent().is_some() {
            Token::new(TokenKind::Dedent, TextRange::empty(self.offset()))
        } else {
            Token::new(TokenKind::EndOfFile, TextRange::empty(self.offset()))
        }
    }

    // Dispatch based on the given character.
    fn consume_ascii_character(&mut self, c: char) -> Token {
        let token = match c {
            c if is_ascii_identifier_start(c) => self.lex_identifier(c),
            '0'..='9' => self.lex_number(c),
            '#' => return Token::new(self.lex_comment(), self.token_range()),
            '\'' | '"' => self.lex_string(AnyStringPrefix::default(), c),
            '=' => {
                if self.cursor.eat_char('=') {
                    TokenKind::EqEqual
                } else {
                    self.state = State::AfterEqual;
                    return Token::new(TokenKind::Equal, self.token_range());
                }
            }
            '+' => {
                if self.cursor.eat_char('=') {
                    TokenKind::PlusEqual
                } else {
                    TokenKind::Plus
                }
            }
            '*' => {
                if self.cursor.eat_char('=') {
                    TokenKind::StarEqual
                } else if self.cursor.eat_char('*') {
                    if self.cursor.eat_char('=') {
                        TokenKind::DoubleStarEqual
                    } else {
                        TokenKind::DoubleStar
                    }
                } else {
                    TokenKind::Star
                }
            }

            c @ ('%' | '!')
                if self.mode == Mode::Ipython
                    && self.state.is_after_equal()
                    && self.nesting == 0 =>
            {
                // SAFETY: Safe because `c` has been matched against one of the possible escape command token
                self.lex_ipython_escape_command(IpyEscapeKind::try_from(c).unwrap())
            }

            c @ ('%' | '!' | '?' | '/' | ';' | ',')
                if self.mode == Mode::Ipython && self.state.is_new_logical_line() =>
            {
                let kind = if let Ok(kind) = IpyEscapeKind::try_from([c, self.cursor.first()]) {
                    self.cursor.bump();
                    kind
                } else {
                    // SAFETY: Safe because `c` has been matched against one of the possible escape command token
                    IpyEscapeKind::try_from(c).unwrap()
                };

                self.lex_ipython_escape_command(kind)
            }

            '?' if self.mode == Mode::Ipython => TokenKind::Question,

            '/' => {
                if self.cursor.eat_char('=') {
                    TokenKind::SlashEqual
                } else if self.cursor.eat_char('/') {
                    if self.cursor.eat_char('=') {
                        TokenKind::DoubleSlashEqual
                    } else {
                        TokenKind::DoubleSlash
                    }
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                if self.cursor.eat_char('=') {
                    TokenKind::PercentEqual
                } else {
                    TokenKind::Percent
                }
            }
            '|' => {
                if self.cursor.eat_char('=') {
                    TokenKind::VbarEqual
                } else {
                    TokenKind::Vbar
                }
            }
            '^' => {
                if self.cursor.eat_char('=') {
                    TokenKind::CircumflexEqual
                } else {
                    TokenKind::CircumFlex
                }
            }
            '&' => {
                if self.cursor.eat_char('=') {
                    TokenKind::AmperEqual
                } else {
                    TokenKind::Amper
                }
            }
            '-' => {
                if self.cursor.eat_char('=') {
                    TokenKind::MinusEqual
                } else if self.cursor.eat_char('>') {
                    TokenKind::Rarrow
                } else {
                    TokenKind::Minus
                }
            }
            '@' => {
                if self.cursor.eat_char('=') {
                    TokenKind::AtEqual
                } else {
                    TokenKind::At
                }
            }
            '!' => {
                if self.cursor.eat_char('=') {
                    TokenKind::NotEqual
                } else {
                    TokenKind::Exclamation
                }
            }
            '~' => TokenKind::Tilde,
            '(' => {
                self.nesting += 1;
                TokenKind::Lpar
            }
            ')' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::Rpar
            }
            '[' => {
                self.nesting += 1;
                TokenKind::Lsqb
            }
            ']' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::Rsqb
            }
            '{' => {
                self.nesting += 1;
                TokenKind::Lbrace
            }
            '}' => {
                if let Some(fstring) = self.fstrings.current_mut() {
                    if fstring.nesting() == self.nesting {
                        return self.push_error_with_range(LexicalError::new(
                            LexicalErrorType::FStringError(FStringErrorType::SingleRbrace),
                            self.token_range(),
                        ));
                    }
                    fstring.try_end_format_spec(self.nesting);
                }
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::Rbrace
            }
            ':' => {
                if self
                    .fstrings
                    .current_mut()
                    .is_some_and(|fstring| fstring.try_start_format_spec(self.nesting))
                {
                    TokenKind::Colon
                } else if self.cursor.eat_char('=') {
                    TokenKind::ColonEqual
                } else {
                    TokenKind::Colon
                }
            }
            ';' => TokenKind::Semi,
            '<' => {
                if self.cursor.eat_char('<') {
                    if self.cursor.eat_char('=') {
                        TokenKind::LeftShiftEqual
                    } else {
                        TokenKind::LeftShift
                    }
                } else if self.cursor.eat_char('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }
            '>' => {
                if self.cursor.eat_char('>') {
                    if self.cursor.eat_char('=') {
                        TokenKind::RightShiftEqual
                    } else {
                        TokenKind::RightShift
                    }
                } else if self.cursor.eat_char('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }
            ',' => TokenKind::Comma,
            '.' => {
                if self.cursor.first().is_ascii_digit() {
                    self.lex_decimal_number('.')
                } else if self.cursor.eat_char2('.', '.') {
                    TokenKind::Ellipsis
                } else {
                    TokenKind::Dot
                }
            }
            '\n' => {
                return Token::new(
                    if self.nesting == 0 && !self.state.is_new_logical_line() {
                        self.state = State::AfterNewline;
                        TokenKind::Newline
                    } else {
                        if let Some(fstring) = self.fstrings.current_mut() {
                            fstring.try_end_format_spec(self.nesting);
                        }
                        TokenKind::NonLogicalNewline
                    },
                    self.token_range(),
                )
            }
            '\r' => {
                self.cursor.eat_char('\n');

                return Token::new(
                    if self.nesting == 0 && !self.state.is_new_logical_line() {
                        self.state = State::AfterNewline;
                        TokenKind::Newline
                    } else {
                        if let Some(fstring) = self.fstrings.current_mut() {
                            fstring.try_end_format_spec(self.nesting);
                        }
                        TokenKind::NonLogicalNewline
                    },
                    self.token_range(),
                );
            }

            _ => {
                self.state = State::Other;

                return self.push_error_with_range(LexicalError::new(
                    LexicalErrorType::UnrecognizedToken { tok: c },
                    self.token_range(),
                ));
            }
        };

        self.state = State::Other;

        Token::new(token, self.token_range())
    }

    #[inline]
    fn token_range(&self) -> TextRange {
        let end = self.offset();
        let len = self.cursor.token_len();

        TextRange::at(end - len, len)
    }

    #[inline]
    fn token_text(&self) -> &'src str {
        &self.source[self.token_range()]
    }

    /// Retrieves the current offset of the cursor within the source code.
    // SAFETY: Lexer doesn't allow files larger than 4GB
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn offset(&self) -> TextSize {
        TextSize::new(self.source.len() as u32) - self.cursor.text_len()
    }

    #[inline]
    fn token_start(&self) -> TextSize {
        self.token_range().start()
    }

    /// Takes the token value corresponding to the current token out of the lexer, replacing it
    /// with the default value.
    ///
    /// All the subsequent call to this method without moving the lexer would always return the
    /// default value which is [`TokenValue::None`].
    pub(crate) fn take_value(&mut self) -> TokenValue {
        std::mem::take(&mut self.value)
    }

    /// Creates a checkpoint to which it can later return to using [`Self::rewind`].
    pub(crate) fn checkpoint(&self) -> LexerCheckpoint<'src> {
        LexerCheckpoint {
            value: self.value.clone(),
            current: self.current,
            cursor: self.cursor.clone(),
            state: self.state,
            nesting: self.nesting,
            indentations_checkpoint: self.indentations.checkpoint(),
            pending_indentation: self.pending_indentation,
            fstrings_checkpoint: self.fstrings.checkpoint(),
        }
    }

    /// Restore the lexer to the given checkpoint.
    ///
    /// # Panics
    ///
    /// If the current indentation is less than the indentation at the checkpoint
    /// If the lexer is out of any f-strings it was in at the time of checkpoint
    pub(crate) fn rewind(&mut self, checkpoint: LexerCheckpoint<'src>) {
        self.value = checkpoint.value;
        self.current = checkpoint.current;
        self.cursor = checkpoint.cursor;
        self.state = checkpoint.state;
        self.nesting = checkpoint.nesting;
        self.indentations.rewind(checkpoint.indentations_checkpoint);
        self.pending_indentation = checkpoint.pending_indentation;
        self.fstrings.rewind(checkpoint.fstrings_checkpoint);
    }

    pub fn finish(self) -> Vec<LexicalError> {
        self.errors
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Token {
    /// The kind of the token.
    kind: TokenKind,
    /// The range of the token.
    range: TextRange,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, range: TextRange) -> Self {
        Self { kind, range }
    }

    #[inline]
    pub const fn kind(&self) -> TokenKind {
        self.kind
    }

    pub(crate) const fn is_trivia(self) -> bool {
        matches!(self.kind, TokenKind::Comment | TokenKind::NonLogicalNewline)
    }
}

impl Ranged for Token {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Represents an error that occur during lexing and are
/// returned by the `parse_*` functions in the iterator in the
/// [lexer] implementation.
///
/// [lexer]: crate::lexer
#[derive(Debug, Clone, PartialEq)]
pub struct LexicalError {
    /// The type of error that occurred.
    error: LexicalErrorType,
    /// The location of the error.
    location: TextRange,
}

impl LexicalError {
    /// Creates a new `LexicalError` with the given error type and location.
    pub fn new(error: LexicalErrorType, location: TextRange) -> Self {
        Self { error, location }
    }

    pub fn error(&self) -> &LexicalErrorType {
        &self.error
    }

    pub fn into_error(self) -> LexicalErrorType {
        self.error
    }

    pub fn location(&self) -> TextRange {
        self.location
    }
}

impl std::ops::Deref for LexicalError {
    type Target = LexicalErrorType;

    fn deref(&self) -> &Self::Target {
        self.error()
    }
}

impl std::error::Error for LexicalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error())
    }
}

impl std::fmt::Display for LexicalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} at byte offset {}",
            self.error(),
            u32::from(self.location().start())
        )
    }
}

/// Represents the different types of errors that can occur during lexing.
#[derive(Debug, Clone, PartialEq)]
pub enum LexicalErrorType {
    // TODO: Can probably be removed, the places it is used seem to be able
    // to use the `UnicodeError` variant instead.
    #[doc(hidden)]
    StringError,
    /// A string literal without the closing quote.
    UnclosedStringError,
    /// Decoding of a unicode escape sequence in a string literal failed.
    UnicodeError,
    /// Missing the `{` for unicode escape sequence.
    MissingUnicodeLbrace,
    /// Missing the `}` for unicode escape sequence.
    MissingUnicodeRbrace,
    /// The indentation is not consistent.
    IndentationError,
    /// An unrecognized token was encountered.
    UnrecognizedToken { tok: char },
    /// An f-string error containing the [`FStringErrorType`].
    FStringError(FStringErrorType),
    /// Invalid character encountered in a byte literal.
    InvalidByteLiteral,
    /// An unexpected character was encountered after a line continuation.
    LineContinuationError,
    /// An unexpected end of file was encountered.
    Eof,
    /// An unexpected error occurred.
    OtherError(Box<str>),
}

impl std::error::Error for LexicalErrorType {}

impl std::fmt::Display for LexicalErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LexicalErrorType::StringError => write!(f, "Got unexpected string"),
            LexicalErrorType::FStringError(error) => write!(f, "f-string: {error}"),
            LexicalErrorType::InvalidByteLiteral => {
                write!(f, "bytes can only contain ASCII literal characters")
            }
            LexicalErrorType::UnicodeError => write!(f, "Got unexpected unicode"),
            LexicalErrorType::IndentationError => {
                write!(f, "unindent does not match any outer indentation level")
            }
            LexicalErrorType::UnrecognizedToken { tok } => {
                write!(f, "Got unexpected token {tok}")
            }
            LexicalErrorType::LineContinuationError => {
                write!(f, "unexpected character after line continuation character")
            }
            LexicalErrorType::Eof => write!(f, "unexpected EOF while parsing"),
            LexicalErrorType::OtherError(msg) => write!(f, "{msg}"),
            LexicalErrorType::UnclosedStringError => {
                write!(f, "missing closing quote in string literal")
            }
            LexicalErrorType::MissingUnicodeLbrace => {
                write!(f, "Missing `{{` in Unicode escape sequence")
            }
            LexicalErrorType::MissingUnicodeRbrace => {
                write!(f, "Missing `}}` in Unicode escape sequence")
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) enum TokenValue {
    #[default]
    None,
    /// Token value for a name, commonly known as an identifier.
    ///
    /// Unicode names are NFKC-normalized by the lexer, matching
    /// [the behaviour of Python's lexer](https://docs.python.org/3/reference/lexical_analysis.html#identifiers)
    Name(Box<str>),
    /// Token value for an integer.
    Int(Int),
    /// Token value for a floating point number.
    Float(f64),
    /// Token value for a complex number.
    Complex {
        /// The real part of the complex number.
        real: f64,
        /// The imaginary part of the complex number.
        imag: f64,
    },
    /// Token value for a string.
    String {
        /// The string value.
        value: Box<str>,
        /// Flags that can be queried to determine the quote style
        /// and prefixes of the string
        flags: AnyStringFlags,
    },
    /// Token value for the start of an f-string. This includes the `f`/`F`/`fr` prefix
    /// and the opening quote(s).
    FStringStart(AnyStringFlags),
    /// Token value that includes the portion of text inside the f-string that's not
    /// part of the expression part and isn't an opening or closing brace.
    FStringMiddle {
        /// The string value.
        value: Box<str>,
        /// Flags that can be queried to determine the quote style
        /// and prefixes of the string
        flags: AnyStringFlags,
    },
    /// Token value for `IPython` escape commands. These are recognized by the lexer
    /// only when the mode is [`Mode::Ipython`].
    IpyEscapeCommand {
        /// The magic command value.
        value: Box<str>,
        /// The kind of magic command.
        kind: IpyEscapeKind,
    },
}

pub(crate) struct LexerCheckpoint<'src> {
    value: TokenValue,
    current: Token,
    cursor: Cursor<'src>,
    state: State,
    nesting: u32,
    indentations_checkpoint: IndentationsCheckpoint,
    pending_indentation: Option<Indentation>,
    fstrings_checkpoint: FStringsCheckpoint,
}

#[derive(Copy, Clone, Debug)]
enum State {
    /// Lexer is right at the beginning of the file or after a `Newline` token.
    AfterNewline,

    /// The lexer is at the start of a new logical line but **after** the indentation
    NonEmptyLogicalLine,

    /// Lexer is right after an equal token
    AfterEqual,

    /// Inside of a logical line
    Other,
}

impl State {
    const fn is_after_newline(self) -> bool {
        matches!(self, State::AfterNewline)
    }

    const fn is_new_logical_line(self) -> bool {
        matches!(self, State::AfterNewline | State::NonEmptyLogicalLine)
    }

    const fn is_after_equal(self) -> bool {
        matches!(self, State::AfterEqual)
    }
}

#[derive(Copy, Clone, Debug)]
enum Radix {
    Binary,
    Octal,
    Decimal,
    Hex,
}

impl Radix {
    const fn as_u32(self) -> u32 {
        match self {
            Radix::Binary => 2,
            Radix::Octal => 8,
            Radix::Decimal => 10,
            Radix::Hex => 16,
        }
    }

    const fn is_digit(self, c: char) -> bool {
        match self {
            Radix::Binary => matches!(c, '0'..='1'),
            Radix::Octal => matches!(c, '0'..='7'),
            Radix::Decimal => c.is_ascii_digit(),
            Radix::Hex => c.is_ascii_hexdigit(),
        }
    }
}

const fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '"')
}

const fn is_ascii_identifier_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_')
}

// Checks if the character c is a valid starting character as described
// in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
fn is_unicode_identifier_start(c: char) -> bool {
    is_xid_start(c)
}

/// Checks if the character c is a valid continuation character as described
/// in <https://docs.python.org/3/reference/lexical_analysis.html#identifiers>.
///
/// Additionally, this function also keeps track of whether or not the total
/// identifier is ASCII-only or not by mutably altering a reference to a
/// boolean value passed in.
fn is_identifier_continuation(c: char, identifier_is_ascii_only: &mut bool) -> bool {
    // Arrange things such that ASCII codepoints never
    // result in the slower `is_xid_continue` getting called.
    if c.is_ascii() {
        matches!(c, 'a'..='z' | 'A'..='Z' | '_' | '0'..='9')
    } else {
        *identifier_is_ascii_only = false;
        is_xid_continue(c)
    }
}

/// Returns `true` for [whitespace](https://docs.python.org/3/reference/lexical_analysis.html#whitespace-between-tokens)
/// characters.
///
/// This is the same as `ruff_python_trivia::is_python_whitespace` and is copied
/// here to avoid a circular dependency as `ruff_python_trivia` has a dev-dependency
/// on `ruff_python_lexer`.
const fn is_python_whitespace(c: char) -> bool {
    matches!(
        c,
        // Space, tab, or form-feed
        ' ' | '\t' | '\x0C'
    )
}

enum LexedText<'a> {
    Source { source: &'a str, range: TextRange },
    Owned(String),
}

impl<'a> LexedText<'a> {
    fn new(start: TextSize, source: &'a str) -> Self {
        Self::Source {
            range: TextRange::empty(start),
            source,
        }
    }

    fn push(&mut self, c: char) {
        match self {
            LexedText::Source { range, source } => {
                *range = range.add_end(c.text_len());
                debug_assert!(source[*range].ends_with(c));
            }
            LexedText::Owned(owned) => owned.push(c),
        }
    }

    fn as_str<'b>(&'b self) -> &'b str
    where
        'b: 'a,
    {
        match self {
            LexedText::Source { range, source } => &source[*range],
            LexedText::Owned(owned) => owned,
        }
    }

    fn skip_char(&mut self) {
        match self {
            LexedText::Source { range, source } => {
                *self = LexedText::Owned(source[*range].to_string());
            }
            LexedText::Owned(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    fn lex(source: &str, mode: Mode) -> (Vec<Token>, Vec<LexicalError>) {
        let mut lexer = Lexer::new(source, mode, TextSize::default());
        let mut tokens = Vec::new();
        loop {
            let next = lexer.next_token();
            if next.kind().is_eof() {
                break;
            }
            tokens.push(next);
        }
        (tokens, lexer.finish())
    }

    fn lex_valid(source: &str, mode: Mode) {
        let (tokens, errors) = lex(source, mode);

        if !errors.is_empty() {
            let mut message = "Unexpected lexical errors for a valid program:\n".to_string();
            for error in errors {
                writeln!(&mut message, "{error:?}").unwrap();
            }
            writeln!(&mut message, "Source:\n{source}").unwrap();
            panic!("{message}");
        }

        insta::assert_debug_snapshot!(tokens);
    }

    fn lex_invalid(source: &str, mode: Mode) {
        let (tokens, errors) = lex(source, mode);

        assert!(
            !errors.is_empty(),
            "Expected lexer to generate at least one error for the following source:\n{source}"
        );

        let mut output = String::new();

        writeln!(&mut output, "## Tokens").unwrap();
        writeln!(&mut output, "```\n{tokens:#?}\n```").unwrap();
        writeln!(&mut output, "## Errors").unwrap();
        writeln!(&mut output, "```\n{errors:#?}\n```").unwrap();

        insta::assert_snapshot!(output);
    }

    fn lex_source(source: &str) {
        lex_valid(source, Mode::Module);
    }

    fn lex_jupyter_source(source: &str) {
        lex_valid(source, Mode::Ipython);
    }

    fn ipython_escape_command_line_continuation_eol(eol: &str) {
        let source = format!("%matplotlib \\{eol}  --inline");
        lex_jupyter_source(&source);
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_unix_eol() {
        ipython_escape_command_line_continuation_eol(UNIX_EOL);
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_mac_eol() {
        ipython_escape_command_line_continuation_eol(MAC_EOL);
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_windows_eol() {
        ipython_escape_command_line_continuation_eol(WINDOWS_EOL);
    }

    fn ipython_escape_command_line_continuation_with_eol_and_eof(eol: &str) {
        let source = format!("%matplotlib \\{eol}");
        lex_jupyter_source(&source);
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_with_unix_eol_and_eof() {
        ipython_escape_command_line_continuation_with_eol_and_eof(UNIX_EOL);
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_with_mac_eol_and_eof() {
        ipython_escape_command_line_continuation_with_eol_and_eof(MAC_EOL);
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_with_windows_eol_and_eof() {
        ipython_escape_command_line_continuation_with_eol_and_eof(WINDOWS_EOL);
    }

    #[test]
    fn test_empty_ipython_escape_command() {
        let source = "%\n%%\n!\n!!\n?\n??\n/\n,\n;";
        lex_jupyter_source(source);
    }

    #[test]
    fn test_ipython_escape_command() {
        let source = r"
?foo
??foo
%timeit a = b
%timeit a % 3
%matplotlib \
    --inline
!pwd \
  && ls -a | sed 's/^/\\    /'
!!cd /Users/foo/Library/Application\ Support/
/foo 1 2
,foo 1 2
;foo 1 2
!ls
"
        .trim();
        lex_jupyter_source(source);
    }

    #[test]
    fn test_ipython_help_end_escape_command() {
        let source = r"
?foo?
??   foo?
??   foo  ?
?foo??
??foo??
???foo?
???foo??
??foo???
???foo???
?? \
    foo?
?? \
?
????
%foo?
%foo??
%%foo???
!pwd?"
            .trim();
        lex_jupyter_source(source);
    }

    #[test]
    fn test_ipython_escape_command_indentation() {
        let source = r"
if True:
    %matplotlib \
        --inline"
            .trim();
        lex_jupyter_source(source);
    }

    #[test]
    fn test_ipython_escape_command_assignment() {
        let source = r"
pwd = !pwd
foo = %timeit a = b
bar = %timeit a % 3
baz = %matplotlib \
        inline"
            .trim();
        lex_jupyter_source(source);
    }

    fn assert_no_ipython_escape_command(tokens: &[Token]) {
        for token in tokens {
            if matches!(token.kind(), TokenKind::IpyEscapeCommand) {
                panic!("Unexpected escape command token at {:?}", token.range())
            }
        }
    }

    #[test]
    fn test_ipython_escape_command_not_an_assignment() {
        let source = r"
# Other escape kinds are not valid here (can't test `foo = ?str` because '?' is not a valid token)
foo = /func
foo = ;func
foo = ,func

(foo == %timeit a = b)
(foo := %timeit a = b)
def f(arg=%timeit a = b):
    pass"
            .trim();
        let (tokens, errors) = lex(source, Mode::Ipython);
        assert!(errors.is_empty());
        assert_no_ipython_escape_command(&tokens);
    }

    #[test]
    fn test_numbers() {
        let source =
            "0x2f 0o12 0b1101 0 123 123_45_67_890 0.2 1e+2 2.1e3 2j 2.2j 000 0x995DC9BBDF1939FA 0x995DC9BBDF1939FA995DC9BBDF1939FA";
        lex_source(source);
    }

    #[test]
    fn test_invalid_leading_zero_small() {
        let source = "025";
        lex_invalid(source, Mode::Module);
    }

    #[test]
    fn test_invalid_leading_zero_big() {
        let source =
            "0252222222222222522222222222225222222222222252222222222222522222222222225222222222222";
        lex_invalid(source, Mode::Module);
    }

    #[test]
    fn test_line_comment_long() {
        let source = "99232  # foo".to_string();
        lex_source(&source);
    }

    #[test]
    fn test_line_comment_whitespace() {
        let source = "99232  #  ".to_string();
        lex_source(&source);
    }

    #[test]
    fn test_line_comment_single_whitespace() {
        let source = "99232  # ".to_string();
        lex_source(&source);
    }

    #[test]
    fn test_line_comment_empty() {
        let source = "99232  #".to_string();
        lex_source(&source);
    }

    fn comment_until_eol(eol: &str) {
        let source = format!("123  # Foo{eol}456");
        lex_source(&source);
    }

    #[test]
    fn test_comment_until_unix_eol() {
        comment_until_eol(UNIX_EOL);
    }

    #[test]
    fn test_comment_until_mac_eol() {
        comment_until_eol(MAC_EOL);
    }

    #[test]
    fn test_comment_until_windows_eol() {
        comment_until_eol(WINDOWS_EOL);
    }

    #[test]
    fn test_assignment() {
        let source = r"a_variable = 99 + 2-0";
        lex_source(source);
    }

    fn indentation_with_eol(eol: &str) {
        let source = format!("def foo():{eol}    return 99{eol}{eol}");
        lex_source(&source);
    }

    #[test]
    fn test_indentation_with_unix_eol() {
        indentation_with_eol(UNIX_EOL);
    }

    #[test]
    fn test_indentation_with_mac_eol() {
        indentation_with_eol(MAC_EOL);
    }

    #[test]
    fn test_indentation_with_windows_eol() {
        indentation_with_eol(WINDOWS_EOL);
    }

    fn double_dedent_with_eol(eol: &str) {
        let source = format!("def foo():{eol} if x:{eol}{eol}  return 99{eol}{eol}");
        lex_source(&source);
    }

    #[test]
    fn test_double_dedent_with_unix_eol() {
        double_dedent_with_eol(UNIX_EOL);
    }

    #[test]
    fn test_double_dedent_with_mac_eol() {
        double_dedent_with_eol(MAC_EOL);
    }

    #[test]
    fn test_double_dedent_with_windows_eol() {
        double_dedent_with_eol(WINDOWS_EOL);
    }

    fn double_dedent_with_tabs_eol(eol: &str) {
        let source = format!("def foo():{eol}\tif x:{eol}{eol}\t\t return 99{eol}{eol}");
        lex_source(&source);
    }

    #[test]
    fn test_double_dedent_with_tabs_unix_eol() {
        double_dedent_with_tabs_eol(UNIX_EOL);
    }

    #[test]
    fn test_double_dedent_with_tabs_mac_eol() {
        double_dedent_with_tabs_eol(MAC_EOL);
    }

    #[test]
    fn test_double_dedent_with_tabs_windows_eol() {
        double_dedent_with_tabs_eol(WINDOWS_EOL);
    }

    fn newline_in_brackets_eol(eol: &str) {
        let source = r"x = [

    1,2
,(3,
4,
), {
5,
6,\
7}]
"
        .replace('\n', eol);
        lex_source(&source);
    }

    #[test]
    fn test_newline_in_brackets_unix_eol() {
        newline_in_brackets_eol(UNIX_EOL);
    }

    #[test]
    fn test_newline_in_brackets_mac_eol() {
        newline_in_brackets_eol(MAC_EOL);
    }

    #[test]
    fn test_newline_in_brackets_windows_eol() {
        newline_in_brackets_eol(WINDOWS_EOL);
    }

    #[test]
    fn test_non_logical_newline_in_string_continuation() {
        let source = r"(
    'a'
    'b'

    'c' \
    'd'
)";
        lex_source(source);
    }

    #[test]
    fn test_logical_newline_line_comment() {
        let source = "#Hello\n#World\n";
        lex_source(source);
    }

    #[test]
    fn test_operators() {
        let source = "//////=/ /";
        lex_source(source);
    }

    #[test]
    fn test_string() {
        let source = r#""double" 'single' 'can\'t' "\\\"" '\t\r\n' '\g' r'raw\'' '\420' '\200\0a'"#;
        lex_source(source);
    }

    fn string_continuation_with_eol(eol: &str) {
        let source = format!("\"abc\\{eol}def\"");
        lex_source(&source);
    }

    #[test]
    fn test_string_continuation_with_unix_eol() {
        string_continuation_with_eol(UNIX_EOL);
    }

    #[test]
    fn test_string_continuation_with_mac_eol() {
        string_continuation_with_eol(MAC_EOL);
    }

    #[test]
    fn test_string_continuation_with_windows_eol() {
        string_continuation_with_eol(WINDOWS_EOL);
    }

    #[test]
    fn test_escape_unicode_name() {
        let source = r#""\N{EN SPACE}""#;
        lex_source(source);
    }

    fn get_tokens_only(source: &str) -> Vec<TokenKind> {
        let (tokens, errors) = lex(source, Mode::Module);
        assert!(errors.is_empty());
        tokens.into_iter().map(|token| token.kind()).collect()
    }

    #[test]
    fn test_nfkc_normalization() {
        let source1 = "𝒞 = 500";
        let source2 = "C = 500";
        assert_eq!(get_tokens_only(source1), get_tokens_only(source2));
    }

    fn triple_quoted_eol(eol: &str) {
        let source = format!("\"\"\"{eol} test string{eol} \"\"\"");
        lex_source(&source);
    }

    #[test]
    fn test_triple_quoted_unix_eol() {
        triple_quoted_eol(UNIX_EOL);
    }

    #[test]
    fn test_triple_quoted_mac_eol() {
        triple_quoted_eol(MAC_EOL);
    }

    #[test]
    fn test_triple_quoted_windows_eol() {
        triple_quoted_eol(WINDOWS_EOL);
    }

    // This test case is to just make sure that the lexer doesn't go into
    // infinite loop on invalid input.
    #[test]
    fn test_infinite_loop() {
        let source = "[1";
        lex_invalid(source, Mode::Module);
    }

    /// Emoji identifiers are a non-standard python feature and are not supported by our lexer.
    #[test]
    fn test_emoji_identifier() {
        let source = "🐦";
        lex_invalid(source, Mode::Module);
    }

    #[test]
    fn tet_too_low_dedent() {
        lex_invalid(
            "if True:
    pass
  pass",
            Mode::Module,
        );
    }

    #[test]
    fn test_empty_fstrings() {
        let source = r#"f"" "" F"" f'' '' f"""""" f''''''"#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_prefix() {
        let source = r#"f"" F"" rf"" rF"" Rf"" RF"" fr"" Fr"" fR"" FR"""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring() {
        let source = r#"f"normal {foo} {{another}} {bar} {{{three}}}""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_parentheses() {
        let source = r#"f"{}" f"{{}}" f" {}" f"{{{}}}" f"{{{{}}}}" f" {} {{}} {{{}}} {{{{}}}}  ""#;
        lex_source(source);
    }

    fn fstring_single_quote_escape_eol(eol: &str) {
        let source = format!(r"f'text \{eol} more text'");
        lex_source(&source);
    }

    #[test]
    fn test_fstring_single_quote_escape_unix_eol() {
        fstring_single_quote_escape_eol(UNIX_EOL);
    }

    #[test]
    fn test_fstring_single_quote_escape_mac_eol() {
        fstring_single_quote_escape_eol(MAC_EOL);
    }

    #[test]
    fn test_fstring_single_quote_escape_windows_eol() {
        fstring_single_quote_escape_eol(WINDOWS_EOL);
    }

    #[test]
    fn test_fstring_escape() {
        let source = r#"f"\{x:\"\{x}} \"\"\
 end""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_escape_braces() {
        let source = r"f'\{foo}' f'\\{foo}' f'\{{foo}}' f'\\{{foo}}'";
        lex_source(source);
    }

    #[test]
    fn test_fstring_escape_raw() {
        let source = r#"rf"\{x:\"\{x}} \"\"\
 end""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_named_unicode() {
        let source = r#"f"\N{BULLET} normal \Nope \N""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_named_unicode_raw() {
        let source = r#"rf"\N{BULLET} normal""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_with_named_expression() {
        let source = r#"f"{x:=10} {(x:=10)} {x,{y:=10}} {[x:=10]}""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_with_format_spec() {
        let source = r#"f"{foo:} {x=!s:.3f} {x:.{y}f} {'':*^{1:{1}}} {x:{{1}.pop()}}""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_with_multiline_format_spec() {
        // The last f-string is invalid syntactically but we should still lex it.
        // Note that the `b` is a `Name` token and not a `FStringMiddle` token.
        let source = r"f'''__{
    x:d
}__'''
f'''__{
    x:a
        b
          c
}__'''
f'__{
    x:d
}__'
f'__{
    x:a
        b
}__'
";
        lex_source(source);
    }

    #[test]
    fn test_fstring_conversion() {
        let source = r#"f"{x!s} {x=!r} {x:.3f!r} {{x!r}}""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_nested() {
        let source = r#"f"foo {f"bar {x + f"{wow}"}"} baz" f'foo {f'bar'} some {f"another"}'"#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_expression_multiline() {
        let source = r#"f"first {
    x
        *
            y
} second""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_multiline() {
        let source = r#"f"""
hello
    world
""" f'''
    world
hello
''' f"some {f"""multiline
allowed {x}"""} string""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_comments() {
        let source = r#"f"""
# not a comment { # comment {
    x
} # not a comment
""""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_with_ipy_escape_command() {
        let source = r#"f"foo {!pwd} bar""#;
        lex_source(source);
    }

    #[test]
    fn test_fstring_with_lambda_expression() {
        let source = r#"
f"{lambda x:{x}}"
f"{(lambda x:{x})}"
"#
        .trim();
        lex_source(source);
    }

    #[test]
    fn test_fstring_with_nul_char() {
        let source = r"f'\0'";
        lex_source(source);
    }

    #[test]
    fn test_match_softkeyword_in_notebook() {
        let source = r"match foo:
    case bar:
        pass";
        lex_jupyter_source(source);
    }

    fn lex_fstring_error(source: &str) -> FStringErrorType {
        let (_, errors) = lex(source, Mode::Module);
        match errors
            .into_iter()
            .next()
            .expect("lexer should give at least one error")
            .into_error()
        {
            LexicalErrorType::FStringError(error) => error,
            err => panic!("Expected FStringError: {err:?}"),
        }
    }

    #[test]
    fn test_fstring_error() {
        use FStringErrorType::{SingleRbrace, UnterminatedString, UnterminatedTripleQuotedString};

        assert_eq!(lex_fstring_error("f'}'"), SingleRbrace);
        assert_eq!(lex_fstring_error("f'{{}'"), SingleRbrace);
        assert_eq!(lex_fstring_error("f'{{}}}'"), SingleRbrace);
        assert_eq!(lex_fstring_error("f'foo}'"), SingleRbrace);
        assert_eq!(lex_fstring_error(r"f'\u007b}'"), SingleRbrace);
        assert_eq!(lex_fstring_error("f'{a:b}}'"), SingleRbrace);
        assert_eq!(lex_fstring_error("f'{3:}}>10}'"), SingleRbrace);
        assert_eq!(lex_fstring_error(r"f'\{foo}\}'"), SingleRbrace);

        assert_eq!(lex_fstring_error(r#"f""#), UnterminatedString);
        assert_eq!(lex_fstring_error(r"f'"), UnterminatedString);

        assert_eq!(lex_fstring_error(r#"f""""#), UnterminatedTripleQuotedString);
        assert_eq!(lex_fstring_error(r"f'''"), UnterminatedTripleQuotedString);
        assert_eq!(
            lex_fstring_error(r#"f"""""#),
            UnterminatedTripleQuotedString
        );
        assert_eq!(
            lex_fstring_error(r#"f""""""#),
            UnterminatedTripleQuotedString
        );
    }
}
