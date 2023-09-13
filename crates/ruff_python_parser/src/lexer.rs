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
//! use ruff_python_parser::{lexer::lex, Tok, Mode, StringKind};
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

use std::iter::FusedIterator;
use std::{char, cmp::Ordering, str::FromStr};

use num_bigint::BigInt;
use num_traits::{Num, Zero};
use ruff_python_ast::IpyEscapeKind;
use ruff_text_size::{TextLen, TextRange, TextSize};
use unicode_ident::{is_xid_continue, is_xid_start};

use crate::lexer::cursor::{Cursor, EOF_CHAR};
use crate::lexer::indentation::{Indentation, Indentations};
use crate::{
    soft_keywords::SoftKeywordTransformer,
    string::FStringErrorType,
    token::{StringKind, Tok},
    Mode,
};

mod cursor;
mod indentation;

/// A lexer for Python source code.
pub struct Lexer<'source> {
    // Contains the source code to be lexed.
    cursor: Cursor<'source>,
    source: &'source str,

    state: State,
    // Amount of parenthesis.
    nesting: u32,
    // Indentation levels.
    indentations: Indentations,
    pending_indentation: Option<Indentation>,
    // Lexer mode.
    mode: Mode,
}

/// Contains a Token along with its `range`.
pub type Spanned = (Tok, TextRange);
/// The result of lexing a token.
pub type LexResult = Result<Spanned, LexicalError>;

/// Create a new lexer from a source string.
///
/// # Examples
///
/// ```
/// use ruff_python_parser::{Mode, lexer::lex};
///
/// let source = "def hello(): return 'world'";
/// let lexer = lex(source, Mode::Module);
///
/// for token in lexer {
///    println!("{:?}", token);
/// }
/// ```
#[inline]
pub fn lex(source: &str, mode: Mode) -> SoftKeywordTransformer<Lexer> {
    SoftKeywordTransformer::new(Lexer::new(source, mode), mode)
}

pub struct LexStartsAtIterator<I> {
    start_offset: TextSize,
    inner: I,
}

impl<I> Iterator for LexStartsAtIterator<I>
where
    I: Iterator<Item = LexResult>,
{
    type Item = LexResult;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let result = match self.inner.next()? {
            Ok((tok, range)) => Ok((tok, range + self.start_offset)),
            Err(error) => Err(LexicalError {
                location: error.location + self.start_offset,
                ..error
            }),
        };

        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I> FusedIterator for LexStartsAtIterator<I> where I: Iterator<Item = LexResult> + FusedIterator {}
impl<I> ExactSizeIterator for LexStartsAtIterator<I> where
    I: Iterator<Item = LexResult> + ExactSizeIterator
{
}

/// Create a new lexer from a source string, starting at a given location.
/// You probably want to use [`lex`] instead.
pub fn lex_starts_at(
    source: &str,
    mode: Mode,
    start_offset: TextSize,
) -> LexStartsAtIterator<SoftKeywordTransformer<Lexer>> {
    LexStartsAtIterator {
        start_offset,
        inner: lex(source, mode),
    }
}

impl<'source> Lexer<'source> {
    /// Create a new lexer from T and a starting location. You probably want to use
    /// [`lex`] instead.
    pub fn new(input: &'source str, mode: Mode) -> Self {
        assert!(
            u32::try_from(input.len()).is_ok(),
            "Lexer only supports files with a size up to 4GB"
        );

        let mut lxr = Lexer {
            state: State::AfterNewline,
            nesting: 0,
            indentations: Indentations::default(),
            pending_indentation: None,

            source: input,
            cursor: Cursor::new(input),
            mode,
        };
        // TODO: Handle possible mismatch between BOM and explicit encoding declaration.
        // spell-checker:ignore feff
        lxr.cursor.eat_char('\u{feff}');

        lxr
    }

    /// Lex an identifier. Also used for keywords and string/bytes literals with a prefix.
    fn lex_identifier(&mut self, first: char) -> Result<Tok, LexicalError> {
        // Detect potential string like rb'' b'' f'' u'' r''
        match self.cursor.first() {
            quote @ ('\'' | '"') => {
                if let Ok(string_kind) = StringKind::try_from(first) {
                    self.cursor.bump();
                    return self.lex_string(string_kind, quote);
                }
            }
            second @ ('f' | 'F' | 'r' | 'R' | 'b' | 'B') if is_quote(self.cursor.second()) => {
                self.cursor.bump();

                if let Ok(string_kind) = StringKind::try_from([first, second]) {
                    let quote = self.cursor.bump().unwrap();
                    return self.lex_string(string_kind, quote);
                }
            }
            _ => {}
        }

        self.cursor.eat_while(is_identifier_continuation);

        let text = self.token_text();

        let keyword = match text {
            "False" => Tok::False,
            "None" => Tok::None,
            "True" => Tok::True,
            "and" => Tok::And,
            "as" => Tok::As,
            "assert" => Tok::Assert,
            "async" => Tok::Async,
            "await" => Tok::Await,
            "break" => Tok::Break,
            "case" => Tok::Case,
            "class" => Tok::Class,
            "continue" => Tok::Continue,
            "def" => Tok::Def,
            "del" => Tok::Del,
            "elif" => Tok::Elif,
            "else" => Tok::Else,
            "except" => Tok::Except,
            "finally" => Tok::Finally,
            "for" => Tok::For,
            "from" => Tok::From,
            "global" => Tok::Global,
            "if" => Tok::If,
            "import" => Tok::Import,
            "in" => Tok::In,
            "is" => Tok::Is,
            "lambda" => Tok::Lambda,
            "match" => Tok::Match,
            "nonlocal" => Tok::Nonlocal,
            "not" => Tok::Not,
            "or" => Tok::Or,
            "pass" => Tok::Pass,
            "raise" => Tok::Raise,
            "return" => Tok::Return,
            "try" => Tok::Try,
            "type" => Tok::Type,
            "while" => Tok::While,
            "with" => Tok::With,
            "yield" => Tok::Yield,
            _ => {
                return Ok(Tok::Name {
                    name: text.to_string(),
                })
            }
        };

        Ok(keyword)
    }

    /// Numeric lexing. The feast can start!
    fn lex_number(&mut self, first: char) -> Result<Tok, LexicalError> {
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
    fn lex_number_radix(&mut self, radix: Radix) -> Result<Tok, LexicalError> {
        #[cfg(debug_assertions)]
        debug_assert!(matches!(
            self.cursor.previous().to_ascii_lowercase(),
            'x' | 'o' | 'b'
        ));

        let mut number = LexedText::new(self.offset(), self.source);
        self.radix_run(&mut number, radix);
        let value =
            BigInt::from_str_radix(number.as_str(), radix.as_u32()).map_err(|e| LexicalError {
                error: LexicalErrorType::OtherError(format!("{e:?}")),
                location: self.token_range().start(),
            })?;
        Ok(Tok::Int { value })
    }

    /// Lex a normal number, that is, no octal, hex or binary number.
    fn lex_decimal_number(&mut self, first_digit_or_dot: char) -> Result<Tok, LexicalError> {
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
                return Err(LexicalError {
                    error: LexicalErrorType::OtherError("Invalid Syntax".to_owned()),
                    location: self.offset() - TextSize::new(1),
                });
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
            let value = f64::from_str(number.as_str()).map_err(|_| LexicalError {
                error: LexicalErrorType::OtherError("Invalid decimal literal".to_owned()),
                location: self.token_start(),
            })?;

            // Parse trailing 'j':
            if self.cursor.eat_if(|c| matches!(c, 'j' | 'J')).is_some() {
                Ok(Tok::Complex {
                    real: 0.0,
                    imag: value,
                })
            } else {
                Ok(Tok::Float { value })
            }
        } else {
            // Parse trailing 'j':
            if self.cursor.eat_if(|c| matches!(c, 'j' | 'J')).is_some() {
                let imag = f64::from_str(number.as_str()).unwrap();
                Ok(Tok::Complex { real: 0.0, imag })
            } else {
                let value = number.as_str().parse::<BigInt>().unwrap();
                if start_is_zero && !value.is_zero() {
                    // leading zeros in decimal integer literals are not permitted
                    return Err(LexicalError {
                        error: LexicalErrorType::OtherError("Invalid Token".to_owned()),
                        location: self.token_range().start(),
                    });
                }
                Ok(Tok::Int { value })
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
    fn lex_comment(&mut self) -> Tok {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), '#');

        self.cursor.eat_while(|c| !matches!(c, '\n' | '\r'));

        Tok::Comment(self.token_text().to_string())
    }

    /// Lex a single IPython escape command.
    fn lex_ipython_escape_command(&mut self, escape_kind: IpyEscapeKind) -> Tok {
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
                    return Tok::IpyEscapeCommand { kind, value };
                }
                '\n' | '\r' | EOF_CHAR => {
                    return Tok::IpyEscapeCommand {
                        kind: escape_kind,
                        value,
                    };
                }
                c => {
                    self.cursor.bump();
                    value.push(c);
                }
            }
        }
    }

    /// Lex a string literal.
    fn lex_string(&mut self, kind: StringKind, quote: char) -> Result<Tok, LexicalError> {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), quote);

        // If the next two characters are also the quote character, then we have a triple-quoted
        // string; consume those two characters and ensure that we require a triple-quote to close
        let triple_quoted = self.cursor.eat_char2(quote, quote);

        let value_start = self.offset();

        let value_end = loop {
            match self.cursor.bump() {
                Some('\\') => {
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else {
                        self.cursor.bump();
                    }
                }
                Some('\r' | '\n') if !triple_quoted => {
                    return Err(LexicalError {
                        error: LexicalErrorType::OtherError(
                            "EOL while scanning string literal".to_owned(),
                        ),
                        location: self.offset() - TextSize::new(1),
                    });
                }
                Some(c) if c == quote => {
                    if triple_quoted {
                        if self.cursor.eat_char2(quote, quote) {
                            break self.offset() - TextSize::new(3);
                        }
                    } else {
                        break self.offset() - TextSize::new(1);
                    }
                }

                Some(_) => {}
                None => {
                    return Err(LexicalError {
                        error: if triple_quoted {
                            LexicalErrorType::Eof
                        } else {
                            LexicalErrorType::StringError
                        },
                        location: self.offset(),
                    });
                }
            }
        };

        let tok = Tok::String {
            value: self.source[TextRange::new(value_start, value_end)].to_string(),
            kind,
            triple_quoted,
        };
        Ok(tok)
    }

    // This is the main entry point. Call this function to retrieve the next token.
    // This function is used by the iterator implementation.
    pub fn next_token(&mut self) -> LexResult {
        // Return dedent tokens until the current indentation level matches the indentation of the next token.
        if let Some(indentation) = self.pending_indentation.take() {
            if let Ok(Ordering::Greater) = self.indentations.current().try_compare(indentation) {
                self.pending_indentation = Some(indentation);
                self.indentations.pop();
                return Ok((Tok::Dedent, TextRange::empty(self.offset())));
            }
        }

        if self.state.is_after_newline() {
            if let Some(indentation) = self.eat_indentation()? {
                return Ok(indentation);
            }
        } else {
            self.skip_whitespace()?;
        }

        self.cursor.start_token();
        if let Some(c) = self.cursor.bump() {
            if c.is_ascii() {
                self.consume_ascii_character(c)
            } else if is_unicode_identifier_start(c) {
                let identifier = self.lex_identifier(c)?;
                self.state = State::Other;

                Ok((identifier, self.token_range()))
            } else {
                Err(LexicalError {
                    error: LexicalErrorType::UnrecognizedToken { tok: c },
                    location: self.token_start(),
                })
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
                        return Err(LexicalError {
                            error: LexicalErrorType::Eof,
                            location: self.token_start(),
                        });
                    } else if !self.cursor.eat_char('\n') {
                        return Err(LexicalError {
                            error: LexicalErrorType::LineContinuationError,
                            location: self.token_start(),
                        });
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

    fn eat_indentation(&mut self) -> Result<Option<Spanned>, LexicalError> {
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
                        return Err(LexicalError {
                            error: LexicalErrorType::Eof,
                            location: self.token_start(),
                        });
                    } else if !self.cursor.eat_char('\n') {
                        return Err(LexicalError {
                            error: LexicalErrorType::LineContinuationError,
                            location: self.token_start(),
                        });
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

            if let Some(spanned) = self.handle_indentation(indentation)? {
                // Set to false so that we don't handle indentation on the next call.

                return Ok(Some(spanned));
            }
        }

        Ok(None)
    }

    fn handle_indentation(
        &mut self,
        indentation: Indentation,
    ) -> Result<Option<Spanned>, LexicalError> {
        let token = match self.indentations.current().try_compare(indentation) {
            // Dedent
            Ok(Ordering::Greater) => {
                self.indentations.pop();
                self.pending_indentation = Some(indentation);

                Some((Tok::Dedent, TextRange::empty(self.offset())))
            }

            Ok(Ordering::Equal) => None,

            // Indent
            Ok(Ordering::Less) => {
                self.indentations.push(indentation);
                Some((Tok::Indent, self.token_range()))
            }
            Err(_) => {
                return Err(LexicalError {
                    error: LexicalErrorType::IndentationError,
                    location: self.offset(),
                });
            }
        };

        Ok(token)
    }

    fn consume_end(&mut self) -> Result<Spanned, LexicalError> {
        // We reached end of file.
        // First of all, we need all nestings to be finished.
        if self.nesting > 0 {
            // Reset the nesting to avoid going into infinite loop.
            self.nesting = 0;
            return Err(LexicalError {
                error: LexicalErrorType::Eof,
                location: self.offset(),
            });
        }

        // Next, insert a trailing newline, if required.
        if !self.state.is_new_logical_line() {
            self.state = State::AfterNewline;
            Ok((Tok::Newline, TextRange::empty(self.offset())))
        }
        // Next, flush the indentation stack to zero.
        else if self.indentations.pop().is_some() {
            Ok((Tok::Dedent, TextRange::empty(self.offset())))
        } else {
            Ok((Tok::EndOfFile, TextRange::empty(self.offset())))
        }
    }

    // Dispatch based on the given character.
    fn consume_ascii_character(&mut self, c: char) -> Result<Spanned, LexicalError> {
        let token = match c {
            c if is_ascii_identifier_start(c) => self.lex_identifier(c)?,
            '0'..='9' => self.lex_number(c)?,
            '#' => return Ok((self.lex_comment(), self.token_range())),
            '"' | '\'' => self.lex_string(StringKind::String, c)?,
            '=' => {
                if self.cursor.eat_char('=') {
                    Tok::EqEqual
                } else {
                    self.state = State::AfterEqual;
                    return Ok((Tok::Equal, self.token_range()));
                }
            }
            '+' => {
                if self.cursor.eat_char('=') {
                    Tok::PlusEqual
                } else {
                    Tok::Plus
                }
            }
            '*' => {
                if self.cursor.eat_char('=') {
                    Tok::StarEqual
                } else if self.cursor.eat_char('*') {
                    if self.cursor.eat_char('=') {
                        Tok::DoubleStarEqual
                    } else {
                        Tok::DoubleStar
                    }
                } else {
                    Tok::Star
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

            '?' if self.mode == Mode::Ipython => Tok::Question,

            '/' => {
                if self.cursor.eat_char('=') {
                    Tok::SlashEqual
                } else if self.cursor.eat_char('/') {
                    if self.cursor.eat_char('=') {
                        Tok::DoubleSlashEqual
                    } else {
                        Tok::DoubleSlash
                    }
                } else {
                    Tok::Slash
                }
            }
            '%' => {
                if self.cursor.eat_char('=') {
                    Tok::PercentEqual
                } else {
                    Tok::Percent
                }
            }
            '|' => {
                if self.cursor.eat_char('=') {
                    Tok::VbarEqual
                } else {
                    Tok::Vbar
                }
            }
            '^' => {
                if self.cursor.eat_char('=') {
                    Tok::CircumflexEqual
                } else {
                    Tok::CircumFlex
                }
            }
            '&' => {
                if self.cursor.eat_char('=') {
                    Tok::AmperEqual
                } else {
                    Tok::Amper
                }
            }
            '-' => {
                if self.cursor.eat_char('=') {
                    Tok::MinusEqual
                } else if self.cursor.eat_char('>') {
                    Tok::Rarrow
                } else {
                    Tok::Minus
                }
            }
            '@' => {
                if self.cursor.eat_char('=') {
                    Tok::AtEqual
                } else {
                    Tok::At
                }
            }
            '!' => {
                if self.cursor.eat_char('=') {
                    Tok::NotEqual
                } else {
                    return Err(LexicalError {
                        error: LexicalErrorType::UnrecognizedToken { tok: '!' },
                        location: self.token_start(),
                    });
                }
            }
            '~' => Tok::Tilde,
            '(' => {
                self.nesting += 1;
                Tok::Lpar
            }
            ')' => {
                self.nesting = self.nesting.saturating_sub(1);
                Tok::Rpar
            }
            '[' => {
                self.nesting += 1;
                Tok::Lsqb
            }
            ']' => {
                self.nesting = self.nesting.saturating_sub(1);
                Tok::Rsqb
            }
            '{' => {
                self.nesting += 1;
                Tok::Lbrace
            }
            '}' => {
                self.nesting = self.nesting.saturating_sub(1);
                Tok::Rbrace
            }
            ':' => {
                if self.cursor.eat_char('=') {
                    Tok::ColonEqual
                } else {
                    Tok::Colon
                }
            }
            ';' => Tok::Semi,
            '<' => {
                if self.cursor.eat_char('<') {
                    if self.cursor.eat_char('=') {
                        Tok::LeftShiftEqual
                    } else {
                        Tok::LeftShift
                    }
                } else if self.cursor.eat_char('=') {
                    Tok::LessEqual
                } else {
                    Tok::Less
                }
            }
            '>' => {
                if self.cursor.eat_char('>') {
                    if self.cursor.eat_char('=') {
                        Tok::RightShiftEqual
                    } else {
                        Tok::RightShift
                    }
                } else if self.cursor.eat_char('=') {
                    Tok::GreaterEqual
                } else {
                    Tok::Greater
                }
            }
            ',' => Tok::Comma,
            '.' => {
                if self.cursor.first().is_ascii_digit() {
                    self.lex_decimal_number('.')?
                } else if self.cursor.eat_char2('.', '.') {
                    Tok::Ellipsis
                } else {
                    Tok::Dot
                }
            }
            '\n' => {
                return Ok((
                    if self.nesting == 0 && !self.state.is_new_logical_line() {
                        self.state = State::AfterNewline;
                        Tok::Newline
                    } else {
                        Tok::NonLogicalNewline
                    },
                    self.token_range(),
                ))
            }
            '\r' => {
                self.cursor.eat_char('\n');

                return Ok((
                    if self.nesting == 0 && !self.state.is_new_logical_line() {
                        self.state = State::AfterNewline;
                        Tok::Newline
                    } else {
                        Tok::NonLogicalNewline
                    },
                    self.token_range(),
                ));
            }

            _ => {
                self.state = State::Other;

                return Err(LexicalError {
                    error: LexicalErrorType::UnrecognizedToken { tok: c },
                    location: self.token_start(),
                });
            }
        };

        self.state = State::Other;

        Ok((token, self.token_range()))
    }

    #[inline]
    fn token_range(&self) -> TextRange {
        let end = self.offset();
        let len = self.cursor.token_len();

        TextRange::at(end - len, len)
    }

    #[inline]
    fn token_text(&self) -> &'source str {
        &self.source[self.token_range()]
    }

    // Lexer doesn't allow files larger than 4GB
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn offset(&self) -> TextSize {
        TextSize::new(self.source.len() as u32) - self.cursor.text_len()
    }

    #[inline]
    fn token_start(&self) -> TextSize {
        self.token_range().start()
    }
}

// Implement iterator pattern for Lexer.
// Calling the next element in the iterator will yield the next lexical
// token.
impl Iterator for Lexer<'_> {
    type Item = LexResult;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token();

        match token {
            Ok((Tok::EndOfFile, _)) => None,
            r => Some(r),
        }
    }
}

impl FusedIterator for Lexer<'_> {}

/// Represents an error that occur during lexing and are
/// returned by the `parse_*` functions in the iterator in the
/// [lexer] implementation.
///
/// [lexer]: crate::lexer
#[derive(Debug, PartialEq)]
pub struct LexicalError {
    /// The type of error that occurred.
    pub error: LexicalErrorType,
    /// The location of the error.
    pub location: TextSize,
}

impl LexicalError {
    /// Creates a new `LexicalError` with the given error type and location.
    pub fn new(error: LexicalErrorType, location: TextSize) -> Self {
        Self { error, location }
    }
}

/// Represents the different types of errors that can occur during lexing.
#[derive(Debug, PartialEq)]
pub enum LexicalErrorType {
    // TODO: Can probably be removed, the places it is used seem to be able
    // to use the `UnicodeError` variant instead.
    #[doc(hidden)]
    StringError,
    // TODO: Should take a start/end position to report.
    /// Decoding of a unicode escape sequence in a string literal failed.
    UnicodeError,
    /// The nesting of brackets/braces/parentheses is not balanced.
    NestingError,
    /// The indentation is not consistent.
    IndentationError,
    /// Inconsistent use of tabs and spaces.
    TabError,
    /// Encountered a tab after a space.
    TabsAfterSpaces,
    /// A non-default argument follows a default argument.
    DefaultArgumentError,
    /// A duplicate argument was found in a function definition.
    DuplicateArgumentError(String),
    /// A positional argument follows a keyword argument.
    PositionalArgumentError,
    /// An iterable argument unpacking `*args` follows keyword argument unpacking `**kwargs`.
    UnpackedArgumentError,
    /// A keyword argument was repeated.
    DuplicateKeywordArgumentError(String),
    /// An unrecognized token was encountered.
    UnrecognizedToken { tok: char },
    /// An f-string error containing the [`FStringErrorType`].
    FStringError(FStringErrorType),
    /// An unexpected character was encountered after a line continuation.
    LineContinuationError,
    /// An unexpected end of file was encountered.
    Eof,
    /// An unexpected error occurred.
    OtherError(String),
}

impl std::fmt::Display for LexicalErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LexicalErrorType::StringError => write!(f, "Got unexpected string"),
            LexicalErrorType::FStringError(error) => write!(f, "f-string: {error}"),
            LexicalErrorType::UnicodeError => write!(f, "Got unexpected unicode"),
            LexicalErrorType::NestingError => write!(f, "Got unexpected nesting"),
            LexicalErrorType::IndentationError => {
                write!(f, "unindent does not match any outer indentation level")
            }
            LexicalErrorType::TabError => {
                write!(f, "inconsistent use of tabs and spaces in indentation")
            }
            LexicalErrorType::TabsAfterSpaces => {
                write!(f, "Tabs not allowed as part of indentation after spaces")
            }
            LexicalErrorType::DefaultArgumentError => {
                write!(f, "non-default argument follows default argument")
            }
            LexicalErrorType::DuplicateArgumentError(arg_name) => {
                write!(f, "duplicate argument '{arg_name}' in function definition")
            }
            LexicalErrorType::DuplicateKeywordArgumentError(arg_name) => {
                write!(f, "keyword argument repeated: {arg_name}")
            }
            LexicalErrorType::PositionalArgumentError => {
                write!(f, "positional argument follows keyword argument")
            }
            LexicalErrorType::UnpackedArgumentError => {
                write!(
                    f,
                    "iterable argument unpacking follows keyword argument unpacking"
                )
            }
            LexicalErrorType::UnrecognizedToken { tok } => {
                write!(f, "Got unexpected token {tok}")
            }
            LexicalErrorType::LineContinuationError => {
                write!(f, "unexpected character after line continuation character")
            }
            LexicalErrorType::Eof => write!(f, "unexpected EOF while parsing"),
            LexicalErrorType::OtherError(msg) => write!(f, "{msg}"),
        }
    }
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
            Radix::Hex => matches!(c, '0'..='9' | 'a'..='f' | 'A'..='F'),
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

// Checks if the character c is a valid continuation character as described
// in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
fn is_identifier_continuation(c: char) -> bool {
    match c {
        'a'..='z' | 'A'..='Z' | '_' | '0'..='9' => true,
        c => is_xid_continue(c),
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
    use insta::assert_debug_snapshot;

    use super::*;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    fn lex_source_with_mode(source: &str, mode: Mode) -> Vec<Spanned> {
        let lexer = lex(source, mode);
        lexer.map(std::result::Result::unwrap).collect()
    }

    fn lex_source(source: &str) -> Vec<Spanned> {
        lex_source_with_mode(source, Mode::Module)
    }

    fn lex_jupyter_source(source: &str) -> Vec<Spanned> {
        lex_source_with_mode(source, Mode::Ipython)
    }

    fn ipython_escape_command_line_continuation_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("%matplotlib \\{eol}  --inline");
        lex_jupyter_source(&source)
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_unix_eol() {
        assert_debug_snapshot!(ipython_escape_command_line_continuation_eol(UNIX_EOL));
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_mac_eol() {
        assert_debug_snapshot!(ipython_escape_command_line_continuation_eol(MAC_EOL));
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_windows_eol() {
        assert_debug_snapshot!(ipython_escape_command_line_continuation_eol(WINDOWS_EOL));
    }

    fn ipython_escape_command_line_continuation_with_eol_and_eof(eol: &str) -> Vec<Spanned> {
        let source = format!("%matplotlib \\{eol}");
        lex_jupyter_source(&source)
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_with_unix_eol_and_eof() {
        assert_debug_snapshot!(ipython_escape_command_line_continuation_with_eol_and_eof(
            UNIX_EOL
        ));
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_with_mac_eol_and_eof() {
        assert_debug_snapshot!(ipython_escape_command_line_continuation_with_eol_and_eof(
            MAC_EOL
        ));
    }

    #[test]
    fn test_ipython_escape_command_line_continuation_with_windows_eol_and_eof() {
        assert_debug_snapshot!(ipython_escape_command_line_continuation_with_eol_and_eof(
            WINDOWS_EOL
        ));
    }

    #[test]
    fn test_empty_ipython_escape_command() {
        let source = "%\n%%\n!\n!!\n?\n??\n/\n,\n;";
        assert_debug_snapshot!(lex_jupyter_source(source));
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
        assert_debug_snapshot!(lex_jupyter_source(source));
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
        assert_debug_snapshot!(lex_jupyter_source(source));
    }

    #[test]
    fn test_ipython_escape_command_indentation() {
        let source = r"
if True:
    %matplotlib \
        --inline"
            .trim();
        assert_debug_snapshot!(lex_jupyter_source(source));
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
        assert_debug_snapshot!(lex_jupyter_source(source));
    }

    fn assert_no_ipython_escape_command(tokens: &[Spanned]) {
        for (tok, _) in tokens {
            if let Tok::IpyEscapeCommand { .. } = tok {
                panic!("Unexpected escape command token: {tok:?}")
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
        let tokens = lex_jupyter_source(source);
        assert_no_ipython_escape_command(&tokens);
    }

    #[test]
    fn test_numbers() {
        let source = "0x2f 0o12 0b1101 0 123 123_45_67_890 0.2 1e+2 2.1e3 2j 2.2j";
        assert_debug_snapshot!(lex_source(source));
    }

    #[test]
    fn test_line_comment_long() {
        let source = "99232  # foo".to_string();
        assert_debug_snapshot!(lex_source(&source));
    }

    #[test]
    fn test_line_comment_whitespace() {
        let source = "99232  #  ".to_string();
        assert_debug_snapshot!(lex_source(&source));
    }

    #[test]
    fn test_line_comment_single_whitespace() {
        let source = "99232  # ".to_string();
        assert_debug_snapshot!(lex_source(&source));
    }

    #[test]
    fn test_line_comment_empty() {
        let source = "99232  #".to_string();
        assert_debug_snapshot!(lex_source(&source));
    }

    fn comment_until_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("123  # Foo{eol}456");
        lex_source(&source)
    }

    #[test]
    fn test_comment_until_unix_eol() {
        assert_debug_snapshot!(comment_until_eol(UNIX_EOL));
    }

    #[test]
    fn test_comment_until_mac_eol() {
        assert_debug_snapshot!(comment_until_eol(MAC_EOL));
    }

    #[test]
    fn test_comment_until_windows_eol() {
        assert_debug_snapshot!(comment_until_eol(WINDOWS_EOL));
    }

    #[test]
    fn test_assignment() {
        let source = r"a_variable = 99 + 2-0";
        assert_debug_snapshot!(lex_source(source));
    }

    fn indentation_with_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("def foo():{eol}    return 99{eol}{eol}");
        lex_source(&source)
    }

    #[test]
    fn test_indentation_with_unix_eol() {
        assert_debug_snapshot!(indentation_with_eol(UNIX_EOL));
    }

    #[test]
    fn test_indentation_with_mac_eol() {
        assert_debug_snapshot!(indentation_with_eol(MAC_EOL));
    }

    #[test]
    fn test_indentation_with_windows_eol() {
        assert_debug_snapshot!(indentation_with_eol(WINDOWS_EOL));
    }

    fn double_dedent_with_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("def foo():{eol} if x:{eol}{eol}  return 99{eol}{eol}");
        lex_source(&source)
    }

    #[test]
    fn test_double_dedent_with_unix_eol() {
        assert_debug_snapshot!(double_dedent_with_eol(UNIX_EOL));
    }

    #[test]
    fn test_double_dedent_with_mac_eol() {
        assert_debug_snapshot!(double_dedent_with_eol(MAC_EOL));
    }

    #[test]
    fn test_double_dedent_with_windows_eol() {
        assert_debug_snapshot!(double_dedent_with_eol(WINDOWS_EOL));
    }

    fn double_dedent_with_tabs_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("def foo():{eol}\tif x:{eol}{eol}\t\t return 99{eol}{eol}");
        lex_source(&source)
    }

    #[test]
    fn test_double_dedent_with_tabs_unix_eol() {
        assert_debug_snapshot!(double_dedent_with_tabs_eol(UNIX_EOL));
    }

    #[test]
    fn test_double_dedent_with_tabs_mac_eol() {
        assert_debug_snapshot!(double_dedent_with_tabs_eol(MAC_EOL));
    }

    #[test]
    fn test_double_dedent_with_tabs_windows_eol() {
        assert_debug_snapshot!(double_dedent_with_tabs_eol(WINDOWS_EOL));
    }

    fn newline_in_brackets_eol(eol: &str) -> Vec<Spanned> {
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
        lex_source(&source)
    }

    #[test]
    fn test_newline_in_brackets_unix_eol() {
        assert_debug_snapshot!(newline_in_brackets_eol(UNIX_EOL));
    }

    #[test]
    fn test_newline_in_brackets_mac_eol() {
        assert_debug_snapshot!(newline_in_brackets_eol(MAC_EOL));
    }

    #[test]
    fn test_newline_in_brackets_windows_eol() {
        assert_debug_snapshot!(newline_in_brackets_eol(WINDOWS_EOL));
    }

    #[test]
    fn test_non_logical_newline_in_string_continuation() {
        let source = r"(
    'a'
    'b'

    'c' \
    'd'
)";
        assert_debug_snapshot!(lex_source(source));
    }

    #[test]
    fn test_logical_newline_line_comment() {
        let source = "#Hello\n#World\n";
        assert_debug_snapshot!(lex_source(source));
    }

    #[test]
    fn test_operators() {
        let source = "//////=/ /";
        assert_debug_snapshot!(lex_source(source));
    }

    #[test]
    fn test_string() {
        let source = r#""double" 'single' 'can\'t' "\\\"" '\t\r\n' '\g' r'raw\'' '\420' '\200\0a'"#;
        assert_debug_snapshot!(lex_source(source));
    }

    fn string_continuation_with_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("\"abc\\{eol}def\"");
        lex_source(&source)
    }

    #[test]
    fn test_string_continuation_with_unix_eol() {
        assert_debug_snapshot!(string_continuation_with_eol(UNIX_EOL));
    }

    #[test]
    fn test_string_continuation_with_mac_eol() {
        assert_debug_snapshot!(string_continuation_with_eol(MAC_EOL));
    }

    #[test]
    fn test_string_continuation_with_windows_eol() {
        assert_debug_snapshot!(string_continuation_with_eol(WINDOWS_EOL));
    }

    #[test]
    fn test_escape_unicode_name() {
        let source = r#""\N{EN SPACE}""#;
        assert_debug_snapshot!(lex_source(source));
    }

    fn triple_quoted_eol(eol: &str) -> Vec<Spanned> {
        let source = format!("\"\"\"{eol} test string{eol} \"\"\"");
        lex_source(&source)
    }

    #[test]
    fn test_triple_quoted_unix_eol() {
        assert_debug_snapshot!(triple_quoted_eol(UNIX_EOL));
    }

    #[test]
    fn test_triple_quoted_mac_eol() {
        assert_debug_snapshot!(triple_quoted_eol(MAC_EOL));
    }

    #[test]
    fn test_triple_quoted_windows_eol() {
        assert_debug_snapshot!(triple_quoted_eol(WINDOWS_EOL));
    }

    // This test case is to just make sure that the lexer doesn't go into
    // infinite loop on invalid input.
    #[test]
    fn test_infite_loop() {
        let source = "[1";
        let _ = lex(source, Mode::Module).collect::<Vec<_>>();
    }

    /// Emoji identifiers are a non-standard python feature and are not supported by our lexer.
    #[test]
    fn test_emoji_identifier() {
        let source = "🐦";

        let lexed: Vec<_> = lex(source, Mode::Module).collect();

        match lexed.as_slice() {
            [Err(error)] => {
                assert_eq!(
                    error.error,
                    LexicalErrorType::UnrecognizedToken { tok: '🐦' }
                );
            }
            result => panic!("Expected an error token but found {result:?}"),
        }
    }
}
