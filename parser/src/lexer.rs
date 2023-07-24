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
//! use rustpython_parser::{lexer::lex, Tok, Mode, StringKind};
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
use crate::{
    ast::bigint::BigInt,
    soft_keywords::SoftKeywordTransformer,
    string::FStringErrorType,
    text_size::{TextLen, TextRange, TextSize},
    token::{MagicKind, StringKind, Tok},
    Mode,
};
use log::trace;
use num_traits::{Num, Zero};
use std::{char, cmp::Ordering, ops::Index, slice::SliceIndex, str::FromStr};
use unic_emoji_char::is_emoji_presentation;
use unic_ucd_ident::{is_xid_continue, is_xid_start};

// Indentations are tracked by a stack of indentation levels. IndentationLevel keeps
// track of the number of tabs and spaces at the current level.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct IndentationLevel {
    tabs: u32,
    spaces: u32,
}

impl IndentationLevel {
    fn compare_strict(
        &self,
        other: &IndentationLevel,
        location: TextSize,
    ) -> Result<Ordering, LexicalError> {
        // We only know for sure that we're smaller or bigger if tabs
        // and spaces both differ in the same direction. Otherwise we're
        // dependent on the size of tabs.
        match self.tabs.cmp(&other.tabs) {
            Ordering::Less => {
                if self.spaces <= other.spaces {
                    Ok(Ordering::Less)
                } else {
                    Err(LexicalError {
                        location,
                        error: LexicalErrorType::TabError,
                    })
                }
            }
            Ordering::Greater => {
                if self.spaces >= other.spaces {
                    Ok(Ordering::Greater)
                } else {
                    Err(LexicalError {
                        location,
                        error: LexicalErrorType::TabError,
                    })
                }
            }
            Ordering::Equal => Ok(self.spaces.cmp(&other.spaces)),
        }
    }
}

// The indentations stack is used to keep track of the current indentation level.
// Similar to the CPython implementation, the Indentations stack always has at
// least one level which is never popped. See Reference 2.1.8.
#[derive(Debug)]
struct Indentations {
    indent_stack: Vec<IndentationLevel>,
}

impl Indentations {
    fn is_empty(&self) -> bool {
        self.indent_stack.len() == 1
    }

    fn push(&mut self, indent: IndentationLevel) {
        self.indent_stack.push(indent);
    }

    fn pop(&mut self) -> Option<IndentationLevel> {
        if self.is_empty() {
            return None;
        }
        self.indent_stack.pop()
    }

    fn current(&self) -> &IndentationLevel {
        self.indent_stack
            .last()
            .expect("Indentations must have at least one level")
    }
}

impl Default for Indentations {
    fn default() -> Self {
        Self {
            indent_stack: vec![IndentationLevel::default()],
        }
    }
}

// A CharWindow is a sliding window over an iterator of chars. It is used to
// allow for look-ahead when scanning tokens from the source code.
struct CharWindow<T: Iterator<Item = char>, const N: usize> {
    source: T,
    window: [Option<char>; N],
}

impl<T, const N: usize> CharWindow<T, N>
where
    T: Iterator<Item = char>,
{
    fn new(source: T) -> Self {
        Self {
            source,
            window: [None; N],
        }
    }

    fn slide(&mut self) -> Option<char> {
        self.window.rotate_left(1);
        let next = self.source.next();
        *self.window.last_mut().expect("never empty") = next;
        next
    }
}

impl<T, const N: usize, Idx> Index<Idx> for CharWindow<T, N>
where
    T: Iterator<Item = char>,
    Idx: SliceIndex<[Option<char>]>,
{
    type Output = Idx::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.window[index]
    }
}

/// A lexer for Python source code.
pub struct Lexer<T: Iterator<Item = char>> {
    // Contains the source code to be lexed.
    window: CharWindow<T, 3>,
    // Are we at the beginning of a line?
    at_begin_of_line: bool,
    // Amount of parenthesis.
    nesting: usize,
    // Indentation levels.
    indentations: Indentations,
    // Pending list of tokens to be returned.
    pending: Vec<Spanned>,
    // The current location.
    location: TextSize,
    // Is the last token an equal sign?
    last_token_is_equal: bool,
    // Lexer mode.
    mode: Mode,
}

// generated in build.rs, in gen_phf()
/// A map of keywords to their tokens.
pub static KEYWORDS: phf::Map<&'static str, Tok> =
    include!(concat!(env!("OUT_DIR"), "/keywords.rs"));

/// Contains a Token along with its `range`.
pub type Spanned = (Tok, TextRange);
/// The result of lexing a token.
pub type LexResult = Result<Spanned, LexicalError>;

/// Create a new lexer from a source string.
///
/// # Examples
///
/// ```
/// use rustpython_parser::{Mode, lexer::lex};
///
/// let source = "def hello(): return 'world'";
/// let lexer = lex(source, Mode::Module);
///
/// for token in lexer {
///    println!("{:?}", token);
/// }
/// ```
#[inline]
pub fn lex(source: &str, mode: Mode) -> impl Iterator<Item = LexResult> + '_ {
    lex_starts_at(source, mode, TextSize::default())
}

/// Create a new lexer from a source string, starting at a given location.
/// You probably want to use [`lex`] instead.
pub fn lex_starts_at(
    source: &str,
    mode: Mode,
    start_offset: TextSize,
) -> SoftKeywordTransformer<Lexer<std::str::Chars<'_>>> {
    SoftKeywordTransformer::new(Lexer::new(source.chars(), mode, start_offset), mode)
}

impl<T> Lexer<T>
where
    T: Iterator<Item = char>,
{
    /// Create a new lexer from T and a starting location. You probably want to use
    /// [`lex`] instead.
    pub fn new(input: T, mode: Mode, start: TextSize) -> Self {
        let mut lxr = Lexer {
            at_begin_of_line: true,
            nesting: 0,
            indentations: Indentations::default(),
            // Usually we have less than 5 tokens pending.
            pending: Vec::with_capacity(5),
            location: start,
            window: CharWindow::new(input),
            last_token_is_equal: false,
            mode,
        };
        // Fill the window.
        lxr.window.slide();
        lxr.window.slide();
        lxr.window.slide();
        // TODO: Handle possible mismatch between BOM and explicit encoding declaration.
        // spell-checker:ignore feff
        if let Some('\u{feff}') = lxr.window[0] {
            lxr.window.slide();
            lxr.location += '\u{feff}'.text_len();
        }
        lxr
    }

    /// Lex an identifier. Also used for keywords and string/bytes literals with a prefix.
    fn lex_identifier(&mut self) -> LexResult {
        // Detect potential string like rb'' b'' f'' u'' r''
        match self.window[..3] {
            [Some(c), Some('"' | '\''), ..] => {
                if let Ok(kind) = StringKind::try_from(c) {
                    return self.lex_string(kind);
                }
            }
            [Some(c1), Some(c2), Some('"' | '\'')] => {
                if let Ok(kind) = StringKind::try_from([c1, c2]) {
                    return self.lex_string(kind);
                }
            }
            _ => {}
        };

        let start_pos = self.get_pos();
        let mut name = String::with_capacity(8);
        while self.is_identifier_continuation() {
            name.push(self.next_char().unwrap());
        }
        let end_pos = self.get_pos();

        if let Some(tok) = KEYWORDS.get(&name) {
            Ok((tok.clone(), TextRange::new(start_pos, end_pos)))
        } else {
            Ok((Tok::Name { name }, TextRange::new(start_pos, end_pos)))
        }
    }

    /// Numeric lexing. The feast can start!
    fn lex_number(&mut self) -> LexResult {
        let start_pos = self.get_pos();
        match self.window[..2] {
            [Some('0'), Some('x' | 'X')] => {
                // Hex! (0xdeadbeef)
                self.next_char();
                self.next_char();
                self.lex_number_radix(start_pos, 16)
            }
            [Some('0'), Some('o' | 'O')] => {
                // Octal style! (0o377)
                self.next_char();
                self.next_char();
                self.lex_number_radix(start_pos, 8)
            }
            [Some('0'), Some('b' | 'B')] => {
                // Binary! (0b_1110_0101)
                self.next_char();
                self.next_char();
                self.lex_number_radix(start_pos, 2)
            }
            _ => self.lex_normal_number(),
        }
    }

    /// Lex a hex/octal/decimal/binary number without a decimal point.
    fn lex_number_radix(&mut self, start_pos: TextSize, radix: u32) -> LexResult {
        let value_text = self.radix_run(radix);
        let end_pos = self.get_pos();
        let value = BigInt::from_str_radix(&value_text, radix).map_err(|e| LexicalError {
            error: LexicalErrorType::OtherError(format!("{e:?}")),
            location: start_pos,
        })?;
        Ok((Tok::Int { value }, TextRange::new(start_pos, end_pos)))
    }

    /// Lex a normal number, that is, no octal, hex or binary number.
    fn lex_normal_number(&mut self) -> LexResult {
        let start_pos = self.get_pos();
        let start_is_zero = self.window[0] == Some('0');
        // Normal number:
        let mut value_text = self.radix_run(10);

        // If float:
        if self.window[0] == Some('.') || self.at_exponent() {
            // Take '.':
            if self.window[0] == Some('.') {
                if self.window[1] == Some('_') {
                    return Err(LexicalError {
                        error: LexicalErrorType::OtherError("Invalid Syntax".to_owned()),
                        location: self.get_pos(),
                    });
                }
                value_text.push(self.next_char().unwrap());
                value_text.push_str(&self.radix_run(10));
            }

            // 1e6 for example:
            if let Some('e' | 'E') = self.window[0] {
                if self.window[1] == Some('_') {
                    return Err(LexicalError {
                        error: LexicalErrorType::OtherError("Invalid Syntax".to_owned()),
                        location: self.get_pos(),
                    });
                }
                value_text.push(self.next_char().unwrap().to_ascii_lowercase());
                // Optional +/-
                if matches!(self.window[0], Some('-' | '+')) {
                    if self.window[1] == Some('_') {
                        return Err(LexicalError {
                            error: LexicalErrorType::OtherError("Invalid Syntax".to_owned()),
                            location: self.get_pos(),
                        });
                    }
                    value_text.push(self.next_char().unwrap());
                }

                value_text.push_str(&self.radix_run(10));
            }

            let value = f64::from_str(&value_text).map_err(|_| LexicalError {
                error: LexicalErrorType::OtherError("Invalid decimal literal".to_owned()),
                location: self.get_pos(),
            })?;

            // Parse trailing 'j':
            if matches!(self.window[0], Some('j' | 'J')) {
                self.next_char();
                let end_pos = self.get_pos();
                Ok((
                    Tok::Complex {
                        real: 0.0,
                        imag: value,
                    },
                    TextRange::new(start_pos, end_pos),
                ))
            } else {
                let end_pos = self.get_pos();
                Ok((Tok::Float { value }, TextRange::new(start_pos, end_pos)))
            }
        } else {
            // Parse trailing 'j':
            if matches!(self.window[0], Some('j' | 'J')) {
                self.next_char();
                let end_pos = self.get_pos();
                let imag = f64::from_str(&value_text).unwrap();
                Ok((
                    Tok::Complex { real: 0.0, imag },
                    TextRange::new(start_pos, end_pos),
                ))
            } else {
                let end_pos = self.get_pos();
                let value = value_text.parse::<BigInt>().unwrap();
                if start_is_zero && !value.is_zero() {
                    // leading zeros in decimal integer literals are not permitted
                    return Err(LexicalError {
                        error: LexicalErrorType::OtherError("Invalid Token".to_owned()),
                        location: self.get_pos(),
                    });
                }
                Ok((Tok::Int { value }, TextRange::new(start_pos, end_pos)))
            }
        }
    }

    /// Consume a sequence of numbers with the given radix,
    /// the digits can be decorated with underscores
    /// like this: '1_2_3_4' == '1234'
    fn radix_run(&mut self, radix: u32) -> String {
        let mut value_text = String::new();

        loop {
            if let Some(c) = self.take_number(radix) {
                value_text.push(c);
            } else if self.window[0] == Some('_')
                && Lexer::<T>::is_digit_of_radix(self.window[1], radix)
            {
                self.next_char();
            } else {
                break;
            }
        }
        value_text
    }

    /// Consume a single character with the given radix.
    fn take_number(&mut self, radix: u32) -> Option<char> {
        let take_char = Lexer::<T>::is_digit_of_radix(self.window[0], radix);

        take_char.then(|| self.next_char().unwrap())
    }

    /// Test if a digit is of a certain radix.
    fn is_digit_of_radix(c: Option<char>, radix: u32) -> bool {
        match radix {
            2 => matches!(c, Some('0'..='1')),
            8 => matches!(c, Some('0'..='7')),
            10 => matches!(c, Some('0'..='9')),
            16 => matches!(c, Some('0'..='9') | Some('a'..='f') | Some('A'..='F')),
            other => unimplemented!("Radix not implemented: {}", other),
        }
    }

    /// Test if we face '[eE][-+]?[0-9]+'
    fn at_exponent(&self) -> bool {
        match self.window[..2] {
            [Some('e' | 'E'), Some('+' | '-')] => matches!(self.window[2], Some('0'..='9')),
            [Some('e' | 'E'), Some('0'..='9')] => true,
            _ => false,
        }
    }

    /// Lex a single comment.
    #[cfg(feature = "full-lexer")]
    fn lex_comment(&mut self) -> LexResult {
        let start_pos = self.get_pos();
        let mut value = String::new();
        loop {
            match self.window[0] {
                Some('\n' | '\r') | None => {
                    let end_pos = self.get_pos();
                    return Ok((Tok::Comment(value), TextRange::new(start_pos, end_pos)));
                }
                Some(_) => {}
            }
            value.push(self.next_char().unwrap());
        }
    }

    #[cfg(feature = "full-lexer")]
    fn lex_and_emit_comment(&mut self) -> Result<(), LexicalError> {
        let comment = self.lex_comment()?;
        self.emit(comment);
        Ok(())
    }

    /// Discard comment if full-lexer is not enabled.
    #[cfg(not(feature = "full-lexer"))]
    fn lex_comment(&mut self) {
        loop {
            match self.window[0] {
                Some('\n' | '\r') | None => {
                    return;
                }
                Some(_) => {}
            }
            self.next_char().unwrap();
        }
    }

    #[cfg(not(feature = "full-lexer"))]
    #[inline]
    fn lex_and_emit_comment(&mut self) -> Result<(), LexicalError> {
        self.lex_comment();
        Ok(())
    }

    /// Lex a single magic command.
    fn lex_magic_command(&mut self, kind: MagicKind) -> (Tok, TextRange) {
        let start_pos = self.get_pos();
        for _ in 0..u32::from(kind.prefix_len()) {
            self.next_char();
        }
        let mut value = String::new();
        loop {
            match self.window[0] {
                Some('\\') => {
                    // Only skip the line continuation if it is followed by a newline
                    // otherwise it is a normal backslash which is part of the magic command:
                    //
                    //        Skip this backslash
                    //        v
                    //   !pwd \
                    //      && ls -a | sed 's/^/\\    /'
                    //                          ^^
                    //                          Don't skip these backslashes
                    if matches!(self.window[1], Some('\n' | '\r')) {
                        self.next_char();
                        self.next_char();
                        continue;
                    }
                }
                Some('\n' | '\r') | None => {
                    let end_pos = self.get_pos();
                    return (
                        Tok::MagicCommand { kind, value },
                        TextRange::new(start_pos, end_pos),
                    );
                }
                Some(_) => {}
            }
            value.push(self.next_char().unwrap());
        }
    }

    fn lex_and_emit_magic_command(&mut self) {
        let kind = match self.window[..2] {
            [Some(c1), Some(c2)] => {
                MagicKind::try_from([c1, c2]).map_or_else(|_| MagicKind::try_from(c1), Ok)
            }
            // When the escape character is the last character of the file.
            [Some(c), None] => MagicKind::try_from(c),
            _ => return,
        };
        if let Ok(kind) = kind {
            let magic_command = self.lex_magic_command(kind);
            self.emit(magic_command);
        }
    }

    /// Lex a string literal.
    fn lex_string(&mut self, kind: StringKind) -> LexResult {
        let start_pos = self.get_pos();
        for _ in 0..u32::from(kind.prefix_len()) {
            self.next_char();
        }
        let quote_char = self.next_char().unwrap();
        let mut string_content = String::with_capacity(5);

        // If the next two characters are also the quote character, then we have a triple-quoted
        // string; consume those two characters and ensure that we require a triple-quote to close
        let triple_quoted = if self.window[..2] == [Some(quote_char); 2] {
            self.next_char();
            self.next_char();
            true
        } else {
            false
        };

        loop {
            match self.next_char() {
                Some(c) => {
                    if c == '\\' {
                        if let Some(next_c) = self.next_char() {
                            string_content.push('\\');
                            string_content.push(next_c);
                            continue;
                        }
                    }
                    if c == '\n' && !triple_quoted {
                        return Err(LexicalError {
                            error: LexicalErrorType::OtherError(
                                "EOL while scanning string literal".to_owned(),
                            ),
                            location: self.get_pos(),
                        });
                    }

                    if c == quote_char {
                        if triple_quoted {
                            // Look ahead at the next two characters; if we have two more
                            // quote_chars, it's the end of the string; consume the remaining
                            // closing quotes and break the loop
                            if self.window[..2] == [Some(quote_char); 2] {
                                self.next_char();
                                self.next_char();
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    string_content.push(c);
                }
                None => {
                    return Err(LexicalError {
                        error: if triple_quoted {
                            LexicalErrorType::Eof
                        } else {
                            LexicalErrorType::StringError
                        },
                        location: self.get_pos(),
                    });
                }
            }
        }
        let end_pos = self.get_pos();
        let tok = Tok::String {
            value: string_content,
            kind,
            triple_quoted,
        };
        Ok((tok, TextRange::new(start_pos, end_pos)))
    }

    // Checks if the character c is a valid starting character as described
    // in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
    fn is_identifier_start(&self, c: char) -> bool {
        match c {
            'a'..='z' | 'A'..='Z' | '_' => true,
            _ => is_xid_start(c),
        }
    }

    // Checks if the character c is a valid continuation character as described
    // in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
    fn is_identifier_continuation(&self) -> bool {
        match self.window[0] {
            Some('a'..='z' | 'A'..='Z' | '_' | '0'..='9') => true,
            Some(c) => is_xid_continue(c),
            _ => false,
        }
    }

    // This is the main entry point. Call this function to retrieve the next token.
    // This function is used by the iterator implementation.
    fn inner_next(&mut self) -> LexResult {
        // top loop, keep on processing, until we have something pending.
        while self.pending.is_empty() {
            // Detect indentation levels
            if self.at_begin_of_line {
                self.handle_indentations()?;
            }

            self.consume_normal()?;
        }

        Ok(self.pending.remove(0))
    }

    // Given we are at the start of a line, count the number of spaces and/or tabs until the first character.
    fn eat_indentation(&mut self) -> Result<IndentationLevel, LexicalError> {
        // Determine indentation:
        let mut spaces: u32 = 0;
        let mut tabs: u32 = 0;
        loop {
            match self.window[0] {
                Some(' ') => {
                    /*
                    if tabs != 0 {
                        // Don't allow spaces after tabs as part of indentation.
                        // This is technically stricter than python3 but spaces after
                        // tabs is even more insane than mixing spaces and tabs.
                        return Some(Err(LexicalError {
                            error: LexicalErrorType::OtherError("Spaces not allowed as part of indentation after tabs".to_owned()),
                            location: self.get_pos(),
                        }));
                    }
                    */
                    self.next_char();
                    spaces += 1;
                }
                Some('\t') => {
                    if spaces != 0 {
                        // Don't allow tabs after spaces as part of indentation.
                        // This is technically stricter than python3 but spaces before
                        // tabs is even more insane than mixing spaces and tabs.
                        return Err(LexicalError {
                            error: LexicalErrorType::TabsAfterSpaces,
                            location: self.get_pos(),
                        });
                    }
                    self.next_char();
                    tabs += 1;
                }
                Some('#') => {
                    self.lex_and_emit_comment()?;
                    spaces = 0;
                    tabs = 0;
                }
                // https://github.com/ipython/ipython/blob/635815e8f1ded5b764d66cacc80bbe25e9e2587f/IPython/core/inputtransformer2.py#L345
                Some('%' | '!' | '?' | '/' | ';' | ',') if self.mode == Mode::Jupyter => {
                    self.lex_and_emit_magic_command();
                }
                Some('\x0C') => {
                    // Form feed character!
                    // Reset indentation for the Emacs user.
                    self.next_char();
                    spaces = 0;
                    tabs = 0;
                }
                Some('\n' | '\r') => {
                    // Empty line!
                    #[cfg(feature = "full-lexer")]
                    let tok_start = self.get_pos();
                    self.next_char();
                    #[cfg(feature = "full-lexer")]
                    let tok_end = self.get_pos();
                    #[cfg(feature = "full-lexer")]
                    self.emit((Tok::NonLogicalNewline, TextRange::new(tok_start, tok_end)));
                    spaces = 0;
                    tabs = 0;
                }
                None => {
                    spaces = 0;
                    tabs = 0;
                    break;
                }
                _ => {
                    self.at_begin_of_line = false;
                    break;
                }
            }
        }

        Ok(IndentationLevel { tabs, spaces })
    }

    // Push/pop indents/dedents based on the current indentation level.
    fn handle_indentations(&mut self) -> Result<(), LexicalError> {
        let indentation_level = self.eat_indentation()?;

        if self.nesting != 0 {
            return Ok(());
        }

        // Determine indent or dedent:
        let current_indentation = self.indentations.current();
        let ordering = indentation_level.compare_strict(current_indentation, self.get_pos())?;
        match ordering {
            Ordering::Equal => {
                // Same same
            }
            Ordering::Greater => {
                // New indentation level:
                self.indentations.push(indentation_level);
                let tok_pos = self.get_pos();
                self.emit((
                    Tok::Indent,
                    TextRange::new(
                        tok_pos
                            - TextSize::new(indentation_level.spaces)
                            - TextSize::new(indentation_level.tabs),
                        tok_pos,
                    ),
                ));
            }
            Ordering::Less => {
                // One or more dedentations
                // Pop off other levels until col is found:

                loop {
                    let current_indentation = self.indentations.current();
                    let ordering =
                        indentation_level.compare_strict(current_indentation, self.get_pos())?;
                    match ordering {
                        Ordering::Less => {
                            self.indentations.pop();
                            let tok_pos = self.get_pos();
                            self.emit((Tok::Dedent, TextRange::empty(tok_pos)));
                        }
                        Ordering::Equal => {
                            // We arrived at proper level of indentation.
                            break;
                        }
                        Ordering::Greater => {
                            return Err(LexicalError {
                                error: LexicalErrorType::IndentationError,
                                location: self.get_pos(),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // Take a look at the next character, if any, and decide upon the next steps.
    fn consume_normal(&mut self) -> Result<(), LexicalError> {
        if let Some(c) = self.window[0] {
            // Identifiers are the most common case.
            if self.is_identifier_start(c) {
                let identifier = self.lex_identifier()?;
                self.emit(identifier);
            } else {
                self.consume_character(c)?;
            }
        } else {
            // We reached end of file.
            let tok_pos = self.get_pos();

            // First of all, we need all nestings to be finished.
            if self.nesting > 0 {
                return Err(LexicalError {
                    error: LexicalErrorType::Eof,
                    location: tok_pos,
                });
            }

            // Next, insert a trailing newline, if required.
            if !self.at_begin_of_line {
                self.at_begin_of_line = true;
                self.emit((Tok::Newline, TextRange::empty(tok_pos)));
            }

            // Next, flush the indentation stack to zero.
            while !self.indentations.is_empty() {
                self.indentations.pop();
                self.emit((Tok::Dedent, TextRange::empty(tok_pos)));
            }

            self.emit((Tok::EndOfFile, TextRange::empty(tok_pos)));
        }

        Ok(())
    }

    // Dispatch based on the given character.
    fn consume_character(&mut self, c: char) -> Result<(), LexicalError> {
        match c {
            '0'..='9' => {
                let number = self.lex_number()?;
                self.emit(number);
            }
            '#' => {
                self.lex_and_emit_comment()?;
            }
            '"' | '\'' => {
                let string = self.lex_string(StringKind::String)?;
                self.emit(string);
            }
            '=' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::EqEqual, TextRange::new(tok_start, tok_end)));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Equal, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '+' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((Tok::PlusEqual, TextRange::new(tok_start, tok_end)));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((Tok::Plus, TextRange::new(tok_start, tok_end)));
                }
            }
            '*' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::StarEqual, TextRange::new(tok_start, tok_end)));
                    }
                    Some('*') => {
                        self.next_char();
                        match self.window[0] {
                            Some('=') => {
                                self.next_char();
                                let tok_end = self.get_pos();
                                self.emit((
                                    Tok::DoubleStarEqual,
                                    TextRange::new(tok_start, tok_end),
                                ));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((Tok::DoubleStar, TextRange::new(tok_start, tok_end)));
                            }
                        }
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Star, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '/' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::SlashEqual, TextRange::new(tok_start, tok_end)));
                    }
                    Some('/') => {
                        self.next_char();
                        match self.window[0] {
                            Some('=') => {
                                self.next_char();
                                let tok_end = self.get_pos();
                                self.emit((
                                    Tok::DoubleSlashEqual,
                                    TextRange::new(tok_start, tok_end),
                                ));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((Tok::DoubleSlash, TextRange::new(tok_start, tok_end)));
                            }
                        }
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Slash, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '%' => {
                if self.mode == Mode::Jupyter && self.nesting == 0 && self.last_token_is_equal {
                    self.lex_and_emit_magic_command();
                } else {
                    let tok_start = self.get_pos();
                    self.next_char();
                    if let Some('=') = self.window[0] {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::PercentEqual, TextRange::new(tok_start, tok_end)));
                    } else {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Percent, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '|' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((Tok::VbarEqual, TextRange::new(tok_start, tok_end)));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((Tok::Vbar, TextRange::new(tok_start, tok_end)));
                }
            }
            '^' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((Tok::CircumflexEqual, TextRange::new(tok_start, tok_end)));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((Tok::CircumFlex, TextRange::new(tok_start, tok_end)));
                }
            }
            '&' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((Tok::AmperEqual, TextRange::new(tok_start, tok_end)));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((Tok::Amper, TextRange::new(tok_start, tok_end)));
                }
            }
            '-' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::MinusEqual, TextRange::new(tok_start, tok_end)));
                    }
                    Some('>') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::Rarrow, TextRange::new(tok_start, tok_end)));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Minus, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '@' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((Tok::AtEqual, TextRange::new(tok_start, tok_end)));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((Tok::At, TextRange::new(tok_start, tok_end)));
                }
            }
            '!' => {
                if self.mode == Mode::Jupyter && self.nesting == 0 && self.last_token_is_equal {
                    self.lex_and_emit_magic_command();
                } else {
                    let tok_start = self.get_pos();
                    self.next_char();
                    if let Some('=') = self.window[0] {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::NotEqual, TextRange::new(tok_start, tok_end)));
                    } else {
                        return Err(LexicalError {
                            error: LexicalErrorType::UnrecognizedToken { tok: '!' },
                            location: tok_start,
                        });
                    }
                }
            }
            '~' => {
                self.eat_single_char(Tok::Tilde);
            }
            '(' => {
                self.eat_single_char(Tok::Lpar);
                self.nesting += 1;
            }
            ')' => {
                self.eat_single_char(Tok::Rpar);
                if self.nesting == 0 {
                    return Err(LexicalError {
                        error: LexicalErrorType::NestingError,
                        location: self.get_pos(),
                    });
                }
                self.nesting -= 1;
            }
            '[' => {
                self.eat_single_char(Tok::Lsqb);
                self.nesting += 1;
            }
            ']' => {
                self.eat_single_char(Tok::Rsqb);
                if self.nesting == 0 {
                    return Err(LexicalError {
                        error: LexicalErrorType::NestingError,
                        location: self.get_pos(),
                    });
                }
                self.nesting -= 1;
            }
            '{' => {
                self.eat_single_char(Tok::Lbrace);
                self.nesting += 1;
            }
            '}' => {
                self.eat_single_char(Tok::Rbrace);
                if self.nesting == 0 {
                    return Err(LexicalError {
                        error: LexicalErrorType::NestingError,
                        location: self.get_pos(),
                    });
                }
                self.nesting -= 1;
            }
            ':' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((Tok::ColonEqual, TextRange::new(tok_start, tok_end)));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((Tok::Colon, TextRange::new(tok_start, tok_end)));
                }
            }
            ';' => {
                self.eat_single_char(Tok::Semi);
            }
            '<' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('<') => {
                        self.next_char();
                        match self.window[0] {
                            Some('=') => {
                                self.next_char();
                                let tok_end = self.get_pos();
                                self.emit((
                                    Tok::LeftShiftEqual,
                                    TextRange::new(tok_start, tok_end),
                                ));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((Tok::LeftShift, TextRange::new(tok_start, tok_end)));
                            }
                        }
                    }
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::LessEqual, TextRange::new(tok_start, tok_end)));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Less, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '>' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('>') => {
                        self.next_char();
                        match self.window[0] {
                            Some('=') => {
                                self.next_char();
                                let tok_end = self.get_pos();
                                self.emit((
                                    Tok::RightShiftEqual,
                                    TextRange::new(tok_start, tok_end),
                                ));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((Tok::RightShift, TextRange::new(tok_start, tok_end)));
                            }
                        }
                    }
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::GreaterEqual, TextRange::new(tok_start, tok_end)));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Greater, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            ',' => {
                self.eat_single_char(Tok::Comma);
            }
            '.' => {
                if let Some('0'..='9') = self.window[1] {
                    let number = self.lex_number()?;
                    self.emit(number);
                } else {
                    let tok_start = self.get_pos();
                    self.next_char();
                    if self.window[..2] == [Some('.'); 2] {
                        self.next_char();
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((Tok::Ellipsis, TextRange::new(tok_start, tok_end)));
                    } else {
                        let tok_end = self.get_pos();
                        self.emit((Tok::Dot, TextRange::new(tok_start, tok_end)));
                    }
                }
            }
            '\n' | '\r' => {
                let tok_start = self.get_pos();
                self.next_char();
                let tok_end = self.get_pos();

                // Depending on the nesting level, we emit a logical or
                // non-logical newline:
                if self.nesting == 0 {
                    self.at_begin_of_line = true;
                    self.emit((Tok::Newline, TextRange::new(tok_start, tok_end)));
                } else {
                    #[cfg(feature = "full-lexer")]
                    self.emit((Tok::NonLogicalNewline, TextRange::new(tok_start, tok_end)));
                }
            }
            ' ' | '\t' | '\x0C' => {
                // Skip white-spaces
                self.next_char();
                while let Some(' ' | '\t' | '\x0C') = self.window[0] {
                    self.next_char();
                }
            }
            '\\' => {
                self.next_char();
                match self.window[0] {
                    Some('\n' | '\r') => {
                        self.next_char();
                    }
                    _ => {
                        return Err(LexicalError {
                            error: LexicalErrorType::LineContinuationError,
                            location: self.get_pos(),
                        });
                    }
                }

                if self.window[0].is_none() {
                    return Err(LexicalError {
                        error: LexicalErrorType::Eof,
                        location: self.get_pos(),
                    });
                }
            }
            _ => {
                if is_emoji_presentation(c) {
                    let tok_start = self.get_pos();
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((
                        Tok::Name {
                            name: c.to_string(),
                        },
                        TextRange::new(tok_start, tok_end),
                    ));
                } else {
                    let c = self.next_char();
                    return Err(LexicalError {
                        error: LexicalErrorType::UnrecognizedToken { tok: c.unwrap() },
                        location: self.get_pos(),
                    });
                }
            }
        }

        Ok(())
    }

    // Used by single character tokens to advance the window and emit the correct token.
    fn eat_single_char(&mut self, ty: Tok) {
        let tok_start = self.get_pos();
        self.next_char().unwrap_or_else(|| unsafe {
            // SAFETY: eat_single_char has been called only after a character has been read
            // from the window, so the window is guaranteed to be non-empty.
            std::hint::unreachable_unchecked()
        });
        let tok_end = self.get_pos();
        self.emit((ty, TextRange::new(tok_start, tok_end)));
    }

    // Helper function to go to the next character coming up.
    fn next_char(&mut self) -> Option<char> {
        let mut c = self.window[0];
        self.window.slide();
        match c {
            Some('\r') => {
                if self.window[0] == Some('\n') {
                    self.location += TextSize::from(1);
                    self.window.slide();
                }

                self.location += TextSize::from(1);
                c = Some('\n');
            }
            #[allow(unused_variables)]
            Some(c) => {
                self.location += c.text_len();
            }
            _ => {}
        }
        c
    }

    // Helper function to retrieve the current position.
    fn get_pos(&self) -> TextSize {
        self.location
    }

    // Helper function to emit a lexed token to the queue of tokens.
    fn emit(&mut self, spanned: Spanned) {
        self.last_token_is_equal = matches!(spanned.0, Tok::Equal);
        self.pending.push(spanned);
    }
}

// Implement iterator pattern for Lexer.
// Calling the next element in the iterator will yield the next lexical
// token.
impl<T> Iterator for Lexer<T>
where
    T: Iterator<Item = char>,
{
    type Item = LexResult;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.inner_next();
        trace!(
            "Lex token {:?}, nesting={:?}, indent stack: {:?}",
            token,
            self.nesting,
            self.indentations,
        );

        match token {
            Ok((Tok::EndOfFile, _)) => None,
            r => Some(r),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::bigint::BigInt;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    pub fn lex_source(source: &str) -> Vec<Tok> {
        let lexer = lex(source, Mode::Module);
        lexer.map(|x| x.unwrap().0).collect()
    }

    pub fn lex_jupyter_source(source: &str) -> Vec<Tok> {
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
        let source = format!("%matplotlib \\{}  --inline", eol);
        let tokens = lex_jupyter_source(&source);
        assert_eq!(
            tokens,
            vec![Tok::MagicCommand {
                value: "matplotlib   --inline".to_string(),
                kind: MagicKind::Magic
            },]
        )
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
        let source = format!("%matplotlib \\{}", eol);
        let tokens = lex_jupyter_source(&source);
        assert_eq!(
            tokens,
            vec![Tok::MagicCommand {
                value: "matplotlib ".to_string(),
                kind: MagicKind::Magic
            },]
        )
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
                    value: "".to_string(),
                    kind: MagicKind::Magic,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Magic2,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Shell,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::ShCap,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Help,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Help2,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Paren,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Quote,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "".to_string(),
                    kind: MagicKind::Quote2,
                },
            ]
        )
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
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "foo".to_string(),
                    kind: MagicKind::Help2,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "timeit a = b".to_string(),
                    kind: MagicKind::Magic,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "timeit a % 3".to_string(),
                    kind: MagicKind::Magic,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "matplotlib     --inline".to_string(),
                    kind: MagicKind::Magic,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "pwd   && ls -a | sed 's/^/\\\\    /'".to_string(),
                    kind: MagicKind::Shell,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "cd /Users/foo/Library/Application\\ Support/".to_string(),
                    kind: MagicKind::ShCap,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "foo 1 2".to_string(),
                    kind: MagicKind::Paren,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "foo 1 2".to_string(),
                    kind: MagicKind::Quote,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "foo 1 2".to_string(),
                    kind: MagicKind::Quote2,
                },
                #[cfg(feature = "full-lexer")]
                Tok::NonLogicalNewline,
                Tok::MagicCommand {
                    value: "ls".to_string(),
                    kind: MagicKind::Shell,
                },
            ]
        )
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
        )
    }

    fn assert_no_jupyter_magic(tokens: &[Tok]) {
        for tok in tokens {
            match tok {
                Tok::MagicCommand { .. } => panic!("Unexpected magic command token: {:?}", tok),
                _ => {}
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
                    value: BigInt::from(1234567890),
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
            #[cfg(feature = "full-lexer")]
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
            #[cfg(feature = "full-lexer")]
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
            #[cfg(feature = "full-lexer")]
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
            #[cfg(feature = "full-lexer")]
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
            #[cfg(feature = "full-lexer")]
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
            #[cfg(feature = "full-lexer")]
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
    #[cfg(feature = "full-lexer")]
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
    #[cfg(feature = "full-lexer")]
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

    macro_rules! test_string_continuation {
        ($($name:ident: $eol:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let source = format!("\"abc\\{}def\"", $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        str_tok("abc\\\ndef"),
                        Tok::Newline,
                    ]
                )
            }
        )*
        }
    }

    test_string_continuation! {
        test_string_continuation_windows_eol: WINDOWS_EOL,
        test_string_continuation_mac_eol: MAC_EOL,
        test_string_continuation_unix_eol: UNIX_EOL,
    }

    #[test]
    fn test_escape_unicode_name() {
        let source = r#""\N{EN SPACE}""#;
        let tokens = lex_source(source);
        assert_eq!(tokens, vec![str_tok(r"\N{EN SPACE}"), Tok::Newline])
    }

    macro_rules! test_triple_quoted {
        ($($name:ident: $eol:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let source = format!("\"\"\"{0} test string{0} \"\"\"", $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::String {
                            value: "\n test string\n ".to_owned(),
                            kind: StringKind::String,
                            triple_quoted: true,
                        },
                        Tok::Newline,
                    ]
                )
            }
        )*
        }
    }

    test_triple_quoted! {
        test_triple_quoted_windows_eol: WINDOWS_EOL,
        test_triple_quoted_mac_eol: MAC_EOL,
        test_triple_quoted_unix_eol: UNIX_EOL,
    }
}
