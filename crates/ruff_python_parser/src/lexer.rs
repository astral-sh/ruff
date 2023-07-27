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

use std::borrow::Cow;
use std::iter::FusedIterator;
use std::{char, cmp::Ordering, str::FromStr};

use num_bigint::BigInt;
use num_traits::{Num, Zero};
use ruff_python_ast::MagicKind;
use ruff_text_size::{TextLen, TextRange, TextSize};
use unic_emoji_char::is_emoji_presentation;
use unic_ucd_ident::{is_xid_continue, is_xid_start};

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

        let value_text = self.radix_run(None, radix);
        let value =
            BigInt::from_str_radix(&value_text, radix.as_u32()).map_err(|e| LexicalError {
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

        let mut value_text = if first_digit_or_dot == '.' {
            String::new()
        } else {
            self.radix_run(Some(first_digit_or_dot), Radix::Decimal)
                .into_owned()
        };

        let is_float = if first_digit_or_dot == '.' || self.cursor.eat_char('.') {
            value_text.push('.');

            if self.cursor.eat_char('_') {
                return Err(LexicalError {
                    error: LexicalErrorType::OtherError("Invalid Syntax".to_owned()),
                    location: self.offset() - TextSize::new(1),
                });
            }

            value_text.push_str(&self.radix_run(None, Radix::Decimal));
            true
        } else {
            // Normal number:
            false
        };

        let is_float = match self.cursor.rest().as_bytes() {
            [b'e' | b'E', b'0'..=b'9', ..] | [b'e' | b'E', b'-' | b'+', b'0'..=b'9', ..] => {
                value_text.push('e');
                self.cursor.bump(); // e | E

                if let Some(sign) = self.cursor.eat_if(|c| matches!(c, '+' | '-')) {
                    value_text.push(sign);
                }

                value_text.push_str(&self.radix_run(None, Radix::Decimal));

                true
            }
            _ => is_float,
        };

        if is_float {
            // Improvement: Use `Cow` instead of pushing to value text
            let value = f64::from_str(&value_text).map_err(|_| LexicalError {
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
                let imag = f64::from_str(&value_text).unwrap();
                Ok(Tok::Complex { real: 0.0, imag })
            } else {
                let value = value_text.parse::<BigInt>().unwrap();
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
    fn radix_run(&mut self, first: Option<char>, radix: Radix) -> Cow<'source, str> {
        let start = if let Some(first) = first {
            self.offset() - first.text_len()
        } else {
            self.offset()
        };
        self.cursor.eat_while(|c| radix.is_digit(c));

        let number = &self.source[TextRange::new(start, self.offset())];

        // Number that contains `_` separators. Remove them from the parsed text.
        if radix.is_digit(self.cursor.second()) && self.cursor.eat_char('_') {
            let mut value_text = number.to_string();

            loop {
                if let Some(c) = self.cursor.eat_if(|c| radix.is_digit(c)) {
                    value_text.push(c);
                } else if self.cursor.first() == '_' && radix.is_digit(self.cursor.second()) {
                    // Skip over `_`
                    self.cursor.bump();
                } else {
                    break;
                }
            }

            Cow::Owned(value_text)
        } else {
            Cow::Borrowed(number)
        }
    }

    /// Lex a single comment.
    fn lex_comment(&mut self) -> Tok {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), '#');

        self.cursor.eat_while(|c| !matches!(c, '\n' | '\r'));

        Tok::Comment(self.token_text().to_string())
    }

    /// Lex a single magic command.
    fn lex_magic_command(&mut self, kind: MagicKind) -> Tok {
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
                '\n' | '\r' | EOF_CHAR => {
                    return Tok::MagicCommand { kind, value };
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
        let triple_quoted = if self.cursor.first() == quote && self.cursor.second() == quote {
            self.cursor.bump();
            self.cursor.bump();
            true
        } else {
            false
        };

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
                        if self.cursor.first() == quote && self.cursor.second() == quote {
                            self.cursor.bump();
                            self.cursor.bump();
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

        if self.state.is_after_newline() {
            // Handle indentation if this is a new, not all empty, logical line
            if !matches!(self.cursor.first(), '\n' | '\r' | '#' | EOF_CHAR) {
                self.state = State::NonEmptyLogicalLine;

                if let Some(spanned) = self.handle_indentation(indentation)? {
                    // Set to false so that we don't handle indentation on the next call.

                    return Ok(spanned);
                }
            }
        }

        self.cursor.start_token();
        if let Some(c) = self.cursor.bump() {
            if c.is_ascii() {
                self.consume_ascii_character(c)
            } else if is_unicode_identifier_start(c) {
                let identifier = self.lex_identifier(c)?;
                self.state = State::Other;

                Ok((identifier, self.token_range()))
            } else if is_emoji_presentation(c) {
                self.state = State::Other;

                Ok((
                    Tok::Name {
                        name: c.to_string(),
                    },
                    self.token_range(),
                ))
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
                if self.mode == Mode::Jupyter
                    && self.state.is_after_equal()
                    && self.nesting == 0 =>
            {
                // SAFETY: Safe because `c` has been matched against one of the possible magic command prefix
                self.lex_magic_command(MagicKind::try_from(c).unwrap())
            }

            c @ ('%' | '!' | '?' | '/' | ';' | ',')
                if self.mode == Mode::Jupyter && self.state.is_new_logical_line() =>
            {
                let kind = if let Ok(kind) = MagicKind::try_from([c, self.cursor.first()]) {
                    self.cursor.bump();
                    kind
                } else {
                    // SAFETY: Safe because `c` has been matched against one of the possible magic command prefix
                    MagicKind::try_from(c).unwrap()
                };

                self.lex_magic_command(kind)
            }
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
                } else if self.cursor.first() == '.' && self.cursor.second() == '.' {
                    self.cursor.bump();
                    self.cursor.bump();
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

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use ruff_python_ast::MagicKind;

    use super::*;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    pub(crate) fn lex_source(source: &str) -> Vec<Tok> {
        let lexer = lex(source, Mode::Module);
        lexer.map(|x| x.unwrap().0).collect()
    }

    pub(crate) fn lex_jupyter_source(source: &str) -> Vec<Tok> {
        let lexer = lex(source, Mode::Jupyter);
        lexer.map(|x| x.unwrap().0).collect()
    }

    fn str_tok(s: &str) -> Tok {
        Tok::String {
            value: s.to_owned(),
            kind: StringKind::String,
            triple_quoted: false,
        }
    }

    fn raw_str_tok(s: &str) -> Tok {
        Tok::String {
            value: s.to_owned(),
            kind: StringKind::RawString,
            triple_quoted: false,
        }
    }

    fn assert_jupyter_magic_line_continuation_with_eol(eol: &str) {
        let source = format!("%matplotlib \\{eol}  --inline");
        let tokens = lex_jupyter_source(&source);
        assert_eq!(
            tokens,
            vec![
                Tok::MagicCommand {
                    value: "matplotlib   --inline".to_string(),
                    kind: MagicKind::Magic
                },
                Tok::Newline
            ]
        );
    }

    #[test]
    fn test_jupyter_magic_line_continuation_unix_eol() {
        assert_jupyter_magic_line_continuation_with_eol(UNIX_EOL);
    }

    #[test]
    fn test_jupyter_magic_line_continuation_mac_eol() {
        assert_jupyter_magic_line_continuation_with_eol(MAC_EOL);
    }

    #[test]
    fn test_jupyter_magic_line_continuation_windows_eol() {
        assert_jupyter_magic_line_continuation_with_eol(WINDOWS_EOL);
    }

    fn assert_jupyter_magic_line_continuation_with_eol_and_eof(eol: &str) {
        let source = format!("%matplotlib \\{eol}");
        let tokens = lex_jupyter_source(&source);
        assert_eq!(
            tokens,
            vec![
                Tok::MagicCommand {
                    value: "matplotlib ".to_string(),
                    kind: MagicKind::Magic
                },
                Tok::Newline
            ]
        );
    }

    #[test]
    fn test_jupyter_magic_line_continuation_unix_eol_and_eof() {
        assert_jupyter_magic_line_continuation_with_eol_and_eof(UNIX_EOL);
    }

    #[test]
    fn test_jupyter_magic_line_continuation_mac_eol_and_eof() {
        assert_jupyter_magic_line_continuation_with_eol_and_eof(MAC_EOL);
    }

    #[test]
    fn test_jupyter_magic_line_continuation_windows_eol_and_eof() {
        assert_jupyter_magic_line_continuation_with_eol_and_eof(WINDOWS_EOL);
    }

    #[test]
    fn test_empty_jupyter_magic() {
        let source = "%\n%%\n!\n!!\n?\n??\n/\n,\n;";
        let tokens = lex_jupyter_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Magic2,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Shell,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::ShCap,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Help,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Help2,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Paren,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Quote,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: String::new(),
                    kind: MagicKind::Quote2,
                },
                Tok::Newline,
            ]
        );
    }

    #[test]
    fn test_jupyter_magic() {
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
        let tokens = lex_jupyter_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::MagicCommand {
                    value: "foo".to_string(),
                    kind: MagicKind::Help,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "foo".to_string(),
                    kind: MagicKind::Help2,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "timeit a = b".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "timeit a % 3".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "matplotlib     --inline".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "pwd   && ls -a | sed 's/^/\\\\    /'".to_string(),
                    kind: MagicKind::Shell,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "cd /Users/foo/Library/Application\\ Support/".to_string(),
                    kind: MagicKind::ShCap,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "foo 1 2".to_string(),
                    kind: MagicKind::Paren,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "foo 1 2".to_string(),
                    kind: MagicKind::Quote,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "foo 1 2".to_string(),
                    kind: MagicKind::Quote2,
                },
                Tok::Newline,
                Tok::MagicCommand {
                    value: "ls".to_string(),
                    kind: MagicKind::Shell,
                },
                Tok::Newline,
            ]
        );
    }
    #[test]
    fn test_jupyter_magic_indentation() {
        let source = r"
if True:
    %matplotlib \
        --inline"
            .trim();
        let tokens = lex_jupyter_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::If,
                Tok::True,
                Tok::Colon,
                Tok::Newline,
                Tok::Indent,
                Tok::MagicCommand {
                    value: "matplotlib         --inline".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::Dedent,
            ]
        );
    }

    #[test]
    fn test_jupyter_magic_assignment() {
        let source = r"
pwd = !pwd
foo = %timeit a = b
bar = %timeit a % 3
baz = %matplotlib \
        inline"
            .trim();
        let tokens = lex_jupyter_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Name {
                    name: "pwd".to_string()
                },
                Tok::Equal,
                Tok::MagicCommand {
                    value: "pwd".to_string(),
                    kind: MagicKind::Shell,
                },
                Tok::Newline,
                Tok::Name {
                    name: "foo".to_string()
                },
                Tok::Equal,
                Tok::MagicCommand {
                    value: "timeit a = b".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::Name {
                    name: "bar".to_string()
                },
                Tok::Equal,
                Tok::MagicCommand {
                    value: "timeit a % 3".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
                Tok::Name {
                    name: "baz".to_string()
                },
                Tok::Equal,
                Tok::MagicCommand {
                    value: "matplotlib         inline".to_string(),
                    kind: MagicKind::Magic,
                },
                Tok::Newline,
            ]
        );
    }

    fn assert_no_jupyter_magic(tokens: &[Tok]) {
        for tok in tokens {
            if let Tok::MagicCommand { .. } = tok {
                panic!("Unexpected magic command token: {tok:?}")
            }
        }
    }

    #[test]
    fn test_jupyter_magic_not_an_assignment() {
        let source = r"
# Other magic kinds are not valid here (can't test `foo = ?str` because '?' is not a valid token)
foo = /func
foo = ;func
foo = ,func

(foo == %timeit a = b)
(foo := %timeit a = b)
def f(arg=%timeit a = b):
    pass"
            .trim();
        let tokens = lex_jupyter_source(source);
        assert_no_jupyter_magic(&tokens);
    }

    #[test]
    fn test_numbers() {
        let source = "0x2f 0o12 0b1101 0 123 123_45_67_890 0.2 1e+2 2.1e3 2j 2.2j";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Int {
                    value: BigInt::from(47),
                },
                Tok::Int {
                    value: BigInt::from(10)
                },
                Tok::Int {
                    value: BigInt::from(13),
                },
                Tok::Int {
                    value: BigInt::from(0),
                },
                Tok::Int {
                    value: BigInt::from(123),
                },
                Tok::Int {
                    value: BigInt::from(1_234_567_890),
                },
                Tok::Float { value: 0.2 },
                Tok::Float { value: 100.0 },
                Tok::Float { value: 2100.0 },
                Tok::Complex {
                    real: 0.0,
                    imag: 2.0,
                },
                Tok::Complex {
                    real: 0.0,
                    imag: 2.2,
                },
                Tok::Newline,
            ]
        );
    }

    macro_rules! test_line_comment {
        ($($name:ident: $eol:expr,)*) => {
            $(
            #[test]

            fn $name() {
                let source = format!(r"99232  # {}", $eol);
                let tokens = lex_source(&source);
                assert_eq!(tokens, vec![Tok::Int { value: BigInt::from(99232) }, Tok::Comment(format!("# {}", $eol)), Tok::Newline]);
            }
            )*
        }
    }

    test_line_comment! {
        test_line_comment_long: " foo",
        test_line_comment_whitespace: "  ",
        test_line_comment_single_whitespace: " ",
        test_line_comment_empty: "",
    }

    macro_rules! test_comment_until_eol {
        ($($name:ident: $eol:expr,)*) => {
            $(
            #[test]

            fn $name() {
                let source = format!("123  # Foo{}456", $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::Int { value: BigInt::from(123) },
                        Tok::Comment("# Foo".to_string()),
                        Tok::Newline,
                        Tok::Int { value: BigInt::from(456) },
                        Tok::Newline,
                    ]
                )
            }
            )*
        }
    }

    test_comment_until_eol! {
        test_comment_until_windows_eol: WINDOWS_EOL,
        test_comment_until_mac_eol: MAC_EOL,
        test_comment_until_unix_eol: UNIX_EOL,
    }

    #[test]
    fn test_assignment() {
        let source = r"a_variable = 99 + 2-0";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Name {
                    name: String::from("a_variable"),
                },
                Tok::Equal,
                Tok::Int {
                    value: BigInt::from(99)
                },
                Tok::Plus,
                Tok::Int {
                    value: BigInt::from(2)
                },
                Tok::Minus,
                Tok::Int {
                    value: BigInt::from(0)
                },
                Tok::Newline,
            ]
        );
    }

    macro_rules! test_indentation_with_eol {
        ($($name:ident: $eol:expr,)*) => {
            $(
            #[test]

            fn $name() {
                let source = format!("def foo():{}   return 99{}{}", $eol, $eol, $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::Def,
                        Tok::Name {
                            name: String::from("foo"),
                        },
                        Tok::Lpar,
                        Tok::Rpar,
                        Tok::Colon,
                        Tok::Newline,
                        Tok::Indent,
                        Tok::Return,
                        Tok::Int { value: BigInt::from(99) },
                        Tok::Newline,
                        Tok::NonLogicalNewline,
                        Tok::Dedent,
                    ]
                );
            }
            )*
        };
    }

    test_indentation_with_eol! {
        test_indentation_windows_eol: WINDOWS_EOL,
        test_indentation_mac_eol: MAC_EOL,
        test_indentation_unix_eol: UNIX_EOL,
    }

    macro_rules! test_double_dedent_with_eol {
        ($($name:ident: $eol:expr,)*) => {
        $(
            #[test]

            fn $name() {
                let source = format!("def foo():{} if x:{}{}  return 99{}{}", $eol, $eol, $eol, $eol, $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::Def,
                        Tok::Name {
                            name: String::from("foo"),
                        },
                        Tok::Lpar,
                        Tok::Rpar,
                        Tok::Colon,
                        Tok::Newline,
                        Tok::Indent,
                        Tok::If,
                        Tok::Name {
                            name: String::from("x"),
                        },
                        Tok::Colon,
                        Tok::Newline,
                        Tok::NonLogicalNewline,
                        Tok::Indent,
                        Tok::Return,
                        Tok::Int { value: BigInt::from(99) },
                        Tok::Newline,
                        Tok::NonLogicalNewline,
                        Tok::Dedent,
                        Tok::Dedent,
                    ]
                );
            }
        )*
        }
    }

    macro_rules! test_double_dedent_with_tabs {
        ($($name:ident: $eol:expr,)*) => {
        $(
            #[test]

            fn $name() {
                let source = format!("def foo():{}\tif x:{}{}\t return 99{}{}", $eol, $eol, $eol, $eol, $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::Def,
                        Tok::Name {
                            name: String::from("foo"),
                        },
                        Tok::Lpar,
                        Tok::Rpar,
                        Tok::Colon,
                        Tok::Newline,
                        Tok::Indent,
                        Tok::If,
                        Tok::Name {
                            name: String::from("x"),
                        },
                        Tok::Colon,
                        Tok::Newline,
                        Tok::NonLogicalNewline,
                        Tok::Indent,
                        Tok::Return,
                        Tok::Int { value: BigInt::from(99) },
                        Tok::Newline,
                        Tok::NonLogicalNewline,
                        Tok::Dedent,
                        Tok::Dedent,
                    ]
                );
            }
        )*
        }
    }

    test_double_dedent_with_eol! {
        test_double_dedent_windows_eol: WINDOWS_EOL,
        test_double_dedent_mac_eol: MAC_EOL,
        test_double_dedent_unix_eol: UNIX_EOL,
    }

    test_double_dedent_with_tabs! {
        test_double_dedent_tabs_windows_eol: WINDOWS_EOL,
        test_double_dedent_tabs_mac_eol: MAC_EOL,
        test_double_dedent_tabs_unix_eol: UNIX_EOL,
    }

    macro_rules! test_newline_in_brackets {
        ($($name:ident: $eol:expr,)*) => {
        $(
            #[test]

            fn $name() {
                let source = r"x = [

    1,2
,(3,
4,
), {
5,
6,\
7}]
".replace("\n", $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::Name {
                            name: String::from("x"),
                        },
                        Tok::Equal,
                        Tok::Lsqb,
                        Tok::NonLogicalNewline,
                        Tok::NonLogicalNewline,
                        Tok::Int { value: BigInt::from(1) },
                        Tok::Comma,
                        Tok::Int { value: BigInt::from(2) },
                        Tok::NonLogicalNewline,
                        Tok::Comma,
                        Tok::Lpar,
                        Tok::Int { value: BigInt::from(3) },
                        Tok::Comma,
                        Tok::NonLogicalNewline,
                        Tok::Int { value: BigInt::from(4) },
                        Tok::Comma,
                        Tok::NonLogicalNewline,
                        Tok::Rpar,
                        Tok::Comma,
                        Tok::Lbrace,
                        Tok::NonLogicalNewline,
                        Tok::Int { value: BigInt::from(5) },
                        Tok::Comma,
                        Tok::NonLogicalNewline,
                        Tok::Int { value: BigInt::from(6) },
                        Tok::Comma,
                        // Continuation here - no NonLogicalNewline.
                        Tok::Int { value: BigInt::from(7) },
                        Tok::Rbrace,
                        Tok::Rsqb,
                        Tok::Newline,
                    ]
                );
            }
        )*
        };
    }

    test_newline_in_brackets! {
        test_newline_in_brackets_windows_eol: WINDOWS_EOL,
        test_newline_in_brackets_mac_eol: MAC_EOL,
        test_newline_in_brackets_unix_eol: UNIX_EOL,
    }

    #[test]
    fn test_non_logical_newline_in_string_continuation() {
        let source = r"(
    'a'
    'b'

    'c' \
    'd'
)";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Lpar,
                Tok::NonLogicalNewline,
                str_tok("a"),
                Tok::NonLogicalNewline,
                str_tok("b"),
                Tok::NonLogicalNewline,
                Tok::NonLogicalNewline,
                str_tok("c"),
                str_tok("d"),
                Tok::NonLogicalNewline,
                Tok::Rpar,
                Tok::Newline,
            ]
        );
    }

    #[test]
    fn test_logical_newline_line_comment() {
        let source = "#Hello\n#World\n";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Comment("#Hello".to_owned()),
                Tok::NonLogicalNewline,
                Tok::Comment("#World".to_owned()),
                Tok::NonLogicalNewline,
            ]
        );
    }

    #[test]
    fn test_operators() {
        let source = "//////=/ /";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::DoubleSlash,
                Tok::DoubleSlash,
                Tok::DoubleSlashEqual,
                Tok::Slash,
                Tok::Slash,
                Tok::Newline,
            ]
        );
    }

    #[test]
    fn test_string() {
        let source = r#""double" 'single' 'can\'t' "\\\"" '\t\r\n' '\g' r'raw\'' '\420' '\200\0a'"#;
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                str_tok("double"),
                str_tok("single"),
                str_tok(r"can\'t"),
                str_tok(r#"\\\""#),
                str_tok(r"\t\r\n"),
                str_tok(r"\g"),
                raw_str_tok(r"raw\'"),
                str_tok(r"\420"),
                str_tok(r"\200\0a"),
                Tok::Newline,
            ]
        );
    }

    fn assert_string_continuation_with_eol(eol: &str) {
        let source = format!("\"abc\\{eol}def\"");
        let tokens = lex_source(&source);

        assert_eq!(
            tokens,
            vec![str_tok(&format!("abc\\{eol}def")), Tok::Newline]
        );
    }

    #[test]
    fn test_string_continuation_windows_eol() {
        assert_string_continuation_with_eol(WINDOWS_EOL);
    }

    #[test]
    fn test_string_continuation_mac_eol() {
        assert_string_continuation_with_eol(MAC_EOL);
    }

    #[test]
    fn test_string_continuation_unix_eol() {
        assert_string_continuation_with_eol(UNIX_EOL);
    }

    #[test]
    fn test_escape_unicode_name() {
        let source = r#""\N{EN SPACE}""#;
        let tokens = lex_source(source);
        assert_eq!(tokens, vec![str_tok(r"\N{EN SPACE}"), Tok::Newline]);
    }

    fn assert_triple_quoted(eol: &str) {
        let source = format!("\"\"\"{eol} test string{eol} \"\"\"");
        let tokens = lex_source(&source);
        assert_eq!(
            tokens,
            vec![
                Tok::String {
                    value: format!("{eol} test string{eol} "),
                    kind: StringKind::String,
                    triple_quoted: true,
                },
                Tok::Newline,
            ]
        );
    }

    #[test]
    fn triple_quoted_windows_eol() {
        assert_triple_quoted(WINDOWS_EOL);
    }

    #[test]
    fn triple_quoted_unix_eol() {
        assert_triple_quoted(UNIX_EOL);
    }

    #[test]
    fn triple_quoted_macos_eol() {
        assert_triple_quoted(MAC_EOL);
    }
}
