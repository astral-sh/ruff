//! This module takes care of lexing python source text.
//!
//! This means source code is translated into separate tokens.

use super::token::StringKind;
pub use super::token::Tok;
use crate::ast::Location;
use crate::error::{LexicalError, LexicalErrorType};
use num_bigint::BigInt;
use num_traits::identities::Zero;
use num_traits::Num;
use std::char;
use std::cmp::Ordering;
use std::ops::Index;
use std::slice::SliceIndex;
use std::str::FromStr;
use unic_emoji_char::is_emoji_presentation;
use unic_ucd_ident::{is_xid_continue, is_xid_start};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct IndentationLevel {
    tabs: usize,
    spaces: usize,
}

impl IndentationLevel {
    fn compare_strict(
        &self,
        other: &IndentationLevel,
        location: Location,
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
            .expect("Indetations must have at least one level")
    }
}

impl Default for Indentations {
    fn default() -> Self {
        Self {
            indent_stack: vec![IndentationLevel::default()],
        }
    }
}

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

    fn change_first(&mut self, ch: char) {
        *self.window.first_mut().expect("never empty") = Some(ch);
    }
}

impl<T, const N: usize, Idx> Index<Idx> for CharWindow<T, N>
where
    T: Iterator<Item = char>,
    Idx: SliceIndex<[Option<char>], Output = Option<char>>,
{
    type Output = Option<char>;

    fn index(&self, index: Idx) -> &Self::Output {
        self.window.index(index)
    }
}

pub struct Lexer<T: Iterator<Item = char>> {
    window: CharWindow<T, 3>,

    at_begin_of_line: bool,
    nesting: usize, // Amount of parenthesis
    indentations: Indentations,

    pending: Vec<Spanned>,
    location: Location,
}

// generated in build.rs, in gen_phf()
pub static KEYWORDS: phf::Map<&'static str, Tok> =
    include!(concat!(env!("OUT_DIR"), "/keywords.rs"));

pub type Spanned = (Location, Tok, Location);
pub type LexResult = Result<Spanned, LexicalError>;

#[inline]
pub fn make_tokenizer(source: &str) -> impl Iterator<Item = LexResult> + '_ {
    make_tokenizer_located(source, Location::new(0, 0))
}

pub fn make_tokenizer_located(
    source: &str,
    start_location: Location,
) -> impl Iterator<Item = LexResult> + '_ {
    let nlh = NewlineHandler::new(source.chars());
    Lexer::new(nlh, start_location)
}

// The newline handler is an iterator which collapses different newline
// types into \n always.
pub struct NewlineHandler<T: Iterator<Item = char>> {
    window: CharWindow<T, 2>,
}

impl<T> NewlineHandler<T>
where
    T: Iterator<Item = char>,
{
    pub fn new(source: T) -> Self {
        let mut nlh = NewlineHandler {
            window: CharWindow::new(source),
        };
        nlh.shift();
        nlh.shift();
        nlh
    }

    fn shift(&mut self) -> Option<char> {
        let result = self.window[0];
        self.window.slide();
        result
    }
}

impl<T> Iterator for NewlineHandler<T>
where
    T: Iterator<Item = char>,
{
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        // Collapse \r\n into \n
        loop {
            match (self.window[0], self.window[1]) {
                (Some('\r'), Some('\n')) => {
                    // Windows EOL into \n
                    self.shift();
                }
                (Some('\r'), _) => {
                    // MAC EOL into \n
                    self.window.change_first('\n');
                }
                _ => break,
            }
        }

        self.shift()
    }
}

/// unicode_name2 does not expose `MAX_NAME_LENGTH`, so we replicate that constant here, fix #3798
const MAX_UNICODE_NAME: usize = 88;

impl<T> Lexer<T>
where
    T: Iterator<Item = char>,
{
    pub fn new(input: T, start: Location) -> Self {
        let mut lxr = Lexer {
            at_begin_of_line: true,
            nesting: 0,
            indentations: Indentations::default(),
            pending: Vec::new(),
            location: start,
            window: CharWindow::new(input),
        };
        lxr.window.slide();
        lxr.window.slide();
        lxr.window.slide();
        // Start at top row (=1) left column (=1)
        lxr.location.reset();
        lxr
    }

    // Lexer helper functions:
    fn lex_identifier(&mut self) -> LexResult {
        let mut name = String::new();
        let start_pos = self.get_pos();

        // Detect potential string like rb'' b'' f'' u'' r''
        let mut saw_b = false;
        let mut saw_r = false;
        let mut saw_u = false;
        let mut saw_f = false;
        loop {
            // Detect r"", f"", b"" and u""
            if !(saw_b || saw_u || saw_f) && matches!(self.window[0], Some('b' | 'B')) {
                saw_b = true;
            } else if !(saw_b || saw_r || saw_u || saw_f)
                && matches!(self.window[0], Some('u' | 'U'))
            {
                saw_u = true;
            } else if !(saw_r || saw_u) && matches!(self.window[0], Some('r' | 'R')) {
                saw_r = true;
            } else if !(saw_b || saw_u || saw_f) && matches!(self.window[0], Some('f' | 'F')) {
                saw_f = true;
            } else {
                break;
            }

            // Take up char into name:
            name.push(self.next_char().unwrap());

            // Check if we have a string:
            if matches!(self.window[0], Some('"' | '\'')) {
                return self
                    .lex_string(saw_b, saw_r, saw_u, saw_f)
                    .map(|(_, tok, end_pos)| (start_pos, tok, end_pos));
            }
        }

        while self.is_identifier_continuation() {
            name.push(self.next_char().unwrap());
        }
        let end_pos = self.get_pos();

        if let Some(tok) = KEYWORDS.get(name.as_str()) {
            Ok((start_pos, tok.clone(), end_pos))
        } else {
            Ok((start_pos, Tok::Name { name }, end_pos))
        }
    }

    /// Numeric lexing. The feast can start!
    fn lex_number(&mut self) -> LexResult {
        let start_pos = self.get_pos();
        match (self.window[0], self.window[1]) {
            (Some('0'), Some('x' | 'X')) => {
                // Hex! (0xdeadbeef)
                self.next_char();
                self.next_char();
                self.lex_number_radix(start_pos, 16)
            }
            (Some('0'), Some('o' | 'O')) => {
                // Octal style! (0o377)
                self.next_char();
                self.next_char();
                self.lex_number_radix(start_pos, 8)
            }
            (Some('0'), Some('b' | 'B')) => {
                // Binary! (0b_1110_0101)
                self.next_char();
                self.next_char();
                self.lex_number_radix(start_pos, 2)
            }
            _ => self.lex_normal_number(),
        }
    }

    /// Lex a hex/octal/decimal/binary number without a decimal point.
    fn lex_number_radix(&mut self, start_pos: Location, radix: u32) -> LexResult {
        let value_text = self.radix_run(radix);
        let end_pos = self.get_pos();
        let value = BigInt::from_str_radix(&value_text, radix).map_err(|e| LexicalError {
            error: LexicalErrorType::OtherError(format!("{:?}", e)),
            location: start_pos,
        })?;
        Ok((start_pos, Tok::Int { value }, end_pos))
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
                    start_pos,
                    Tok::Complex {
                        real: 0.0,
                        imag: value,
                    },
                    end_pos,
                ))
            } else {
                let end_pos = self.get_pos();
                Ok((start_pos, Tok::Float { value }, end_pos))
            }
        } else {
            // Parse trailing 'j':
            if matches!(self.window[0], Some('j' | 'J')) {
                self.next_char();
                let end_pos = self.get_pos();
                let imag = f64::from_str(&value_text).unwrap();
                Ok((start_pos, Tok::Complex { real: 0.0, imag }, end_pos))
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
                Ok((start_pos, Tok::Int { value }, end_pos))
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
        match (self.window[0], self.window[1]) {
            (Some('e' | 'E'), Some('+' | '-')) => matches!(self.window[2], Some('0'..='9')),
            (Some('e' | 'E'), Some('0'..='9')) => true,
            _ => false,
        }
    }

    /// Skip everything until end of line
    fn lex_comment(&mut self) -> LexResult {
        let start_pos = self.get_pos();
        self.next_char();
        loop {
            match self.window[0] {
                Some('\n') | None => {
                    let end_pos = self.get_pos();
                    return Ok((start_pos, Tok::Comment, end_pos));
                }
                Some(_) => {}
            }
            self.next_char();
        }
    }

    fn unicode_literal(&mut self, literal_number: usize) -> Result<char, LexicalError> {
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
            if let Some('0'..='7') = self.window[0] {
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

    fn lex_string(
        &mut self,
        is_bytes: bool,
        is_raw: bool,
        is_unicode: bool,
        is_fstring: bool,
    ) -> LexResult {
        let start_pos = self.get_pos();
        let quote_char = self.next_char().unwrap();
        let mut string_content = String::new();

        // If the next two characters are also the quote character, then we have a triple-quoted
        // string; consume those two characters and ensure that we require a triple-quote to close
        let triple_quoted =
            if self.window[0] == Some(quote_char) && self.window[1] == Some(quote_char) {
                self.next_char();
                self.next_char();
                true
            } else {
                false
            };

        loop {
            match self.next_char() {
                Some('\\') => {
                    if self.window[0] == Some(quote_char) && !is_raw {
                        string_content.push(quote_char);
                        self.next_char();
                    } else if is_raw {
                        string_content.push('\\');
                        if let Some(c) = self.next_char() {
                            string_content.push(c)
                        } else {
                            return Err(LexicalError {
                                error: LexicalErrorType::StringError,
                                location: self.get_pos(),
                            });
                        }
                    } else {
                        match self.next_char() {
                            Some('\\') => {
                                string_content.push('\\');
                            }
                            Some('\'') => string_content.push('\''),
                            Some('\"') => string_content.push('\"'),
                            Some('\n') => {
                                // Ignore Unix EOL character
                            }
                            Some('a') => string_content.push('\x07'),
                            Some('b') => string_content.push('\x08'),
                            Some('f') => string_content.push('\x0c'),
                            Some('n') => {
                                string_content.push('\n');
                            }
                            Some('r') => string_content.push('\r'),
                            Some('t') => {
                                string_content.push('\t');
                            }
                            Some('v') => string_content.push('\x0b'),
                            Some(o @ '0'..='7') => string_content.push(self.parse_octet(o)),
                            Some('x') => string_content.push(self.unicode_literal(2)?),
                            Some('u') if !is_bytes => string_content.push(self.unicode_literal(4)?),
                            Some('U') if !is_bytes => string_content.push(self.unicode_literal(8)?),
                            Some('N') if !is_bytes => {
                                string_content.push(self.parse_unicode_name()?)
                            }
                            Some(c) => {
                                string_content.push('\\');
                                string_content.push(c);
                            }
                            None => {
                                return Err(LexicalError {
                                    error: LexicalErrorType::StringError,
                                    location: self.get_pos(),
                                });
                            }
                        }
                    }
                }
                Some(c) => {
                    if c == quote_char {
                        if triple_quoted {
                            // Look ahead at the next two characters; if we have two more
                            // quote_chars, it's the end of the string; consume the remaining
                            // closing quotes and break the loop
                            if self.window[0] == Some(quote_char)
                                && self.window[1] == Some(quote_char)
                            {
                                self.next_char();
                                self.next_char();
                                break;
                            }
                            string_content.push(c);
                        } else {
                            break;
                        }
                    } else {
                        if (c == '\n' && !triple_quoted) || (is_bytes && !c.is_ascii()) {
                            return Err(LexicalError {
                                error: LexicalErrorType::Eof,
                                location: self.get_pos(),
                            });
                        }
                        string_content.push(c);
                    }
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

        let tok = if is_bytes {
            Tok::Bytes {
                value: string_content.chars().map(|c| c as u8).collect(),
            }
        } else {
            let kind = if is_fstring {
                StringKind::F
            } else if is_unicode {
                StringKind::U
            } else {
                StringKind::Normal
            };
            Tok::String {
                value: string_content,
                kind,
            }
        };

        Ok((start_pos, tok, end_pos))
    }

    fn is_identifier_start(&self, c: char) -> bool {
        c == '_' || is_xid_start(c)
    }

    fn is_identifier_continuation(&self) -> bool {
        match self.window[0] {
            Some('_' | '0'..='9') => true,
            Some(c) => is_xid_continue(c),
            _ => false,
        }
    }

    /// This is the main entry point. Call this function to retrieve the next token.
    /// This function is used by the iterator implementation.
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

    /// Given we are at the start of a line, count the number of spaces and/or tabs until the first character.
    fn eat_indentation(&mut self) -> Result<IndentationLevel, LexicalError> {
        // Determine indentation:
        let mut spaces: usize = 0;
        let mut tabs: usize = 0;
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
                    let comment = self.lex_comment()?;
                    self.emit(comment);
                    spaces = 0;
                    tabs = 0;
                }
                Some('\x0C') => {
                    // Form feed character!
                    // Reset indentation for the Emacs user.
                    self.next_char();
                    spaces = 0;
                    tabs = 0;
                }
                Some('\n') => {
                    // Empty line!
                    self.next_char();
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
                self.emit((tok_pos, Tok::Indent, tok_pos));
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
                            self.emit((tok_pos, Tok::Dedent, tok_pos));
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

    /// Take a look at the next character, if any, and decide upon the next steps.
    fn consume_normal(&mut self) -> Result<(), LexicalError> {
        // Check if we have some character:
        if let Some(c) = self.window[0] {
            // First check identifier:
            if self.is_identifier_start(c) {
                let identifier = self.lex_identifier()?;
                self.emit(identifier);
            } else if is_emoji_presentation(c) {
                let tok_start = self.get_pos();
                self.next_char();
                let tok_end = self.get_pos();
                self.emit((
                    tok_start,
                    Tok::Name {
                        name: c.to_string(),
                    },
                    tok_end,
                ));
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
                self.emit((tok_pos, Tok::Newline, tok_pos));
            }

            // Next, flush the indentation stack to zero.
            while !self.indentations.is_empty() {
                self.indentations.pop();
                self.emit((tok_pos, Tok::Dedent, tok_pos));
            }

            self.emit((tok_pos, Tok::EndOfFile, tok_pos));
        }

        Ok(())
    }

    /// Okay, we are facing a weird character, what is it? Determine that.
    fn consume_character(&mut self, c: char) -> Result<(), LexicalError> {
        match c {
            '0'..='9' => {
                let number = self.lex_number()?;
                self.emit(number);
            }
            '#' => {
                let comment = self.lex_comment()?;
                self.emit(comment);
            }
            '"' | '\'' => {
                let string = self.lex_string(false, false, false, false)?;
                self.emit(string);
            }
            '=' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::EqEqual, tok_end));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Equal, tok_end));
                    }
                }
            }
            '+' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::PlusEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::Plus, tok_end));
                }
            }
            '*' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::StarEqual, tok_end));
                    }
                    Some('*') => {
                        self.next_char();
                        match self.window[0] {
                            Some('=') => {
                                self.next_char();
                                let tok_end = self.get_pos();
                                self.emit((tok_start, Tok::DoubleStarEqual, tok_end));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((tok_start, Tok::DoubleStar, tok_end));
                            }
                        }
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Star, tok_end));
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
                        self.emit((tok_start, Tok::SlashEqual, tok_end));
                    }
                    Some('/') => {
                        self.next_char();
                        match self.window[0] {
                            Some('=') => {
                                self.next_char();
                                let tok_end = self.get_pos();
                                self.emit((tok_start, Tok::DoubleSlashEqual, tok_end));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((tok_start, Tok::DoubleSlash, tok_end));
                            }
                        }
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Slash, tok_end));
                    }
                }
            }
            '%' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::PercentEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::Percent, tok_end));
                }
            }
            '|' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::VbarEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::Vbar, tok_end));
                }
            }
            '^' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::CircumflexEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::CircumFlex, tok_end));
                }
            }
            '&' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::AmperEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::Amper, tok_end));
                }
            }
            '-' => {
                let tok_start = self.get_pos();
                self.next_char();
                match self.window[0] {
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::MinusEqual, tok_end));
                    }
                    Some('>') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Rarrow, tok_end));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Minus, tok_end));
                    }
                }
            }
            '@' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::AtEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::At, tok_end));
                }
            }
            '!' => {
                let tok_start = self.get_pos();
                self.next_char();
                if let Some('=') = self.window[0] {
                    self.next_char();
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::NotEqual, tok_end));
                } else {
                    return Err(LexicalError {
                        error: LexicalErrorType::UnrecognizedToken { tok: '!' },
                        location: tok_start,
                    });
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
                    self.emit((tok_start, Tok::ColonEqual, tok_end));
                } else {
                    let tok_end = self.get_pos();
                    self.emit((tok_start, Tok::Colon, tok_end));
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
                                self.emit((tok_start, Tok::LeftShiftEqual, tok_end));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((tok_start, Tok::LeftShift, tok_end));
                            }
                        }
                    }
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::LessEqual, tok_end));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Less, tok_end));
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
                                self.emit((tok_start, Tok::RightShiftEqual, tok_end));
                            }
                            _ => {
                                let tok_end = self.get_pos();
                                self.emit((tok_start, Tok::RightShift, tok_end));
                            }
                        }
                    }
                    Some('=') => {
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::GreaterEqual, tok_end));
                    }
                    _ => {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Greater, tok_end));
                    }
                }
            }
            ',' => {
                let tok_start = self.get_pos();
                self.next_char();
                let tok_end = self.get_pos();
                self.emit((tok_start, Tok::Comma, tok_end));
            }
            '.' => {
                if let Some('0'..='9') = self.window[1] {
                    let number = self.lex_number()?;
                    self.emit(number);
                } else {
                    let tok_start = self.get_pos();
                    self.next_char();
                    if let (Some('.'), Some('.')) = (&self.window[0], &self.window[1]) {
                        self.next_char();
                        self.next_char();
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Ellipsis, tok_end));
                    } else {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Dot, tok_end));
                    }
                }
            }
            '\n' => {
                let tok_start = self.get_pos();
                self.next_char();
                let tok_end = self.get_pos();

                // Depending on the nesting level, we emit newline or not:
                if self.nesting == 0 {
                    self.at_begin_of_line = true;
                    self.emit((tok_start, Tok::Newline, tok_end));
                }
            }
            ' ' | '\t' | '\x0C' => {
                // Skip whitespaces
                self.next_char();
                while let Some(' ' | '\t' | '\x0C') = self.window[0] {
                    self.next_char();
                }
            }
            '\\' => {
                self.next_char();
                if let Some('\n') = self.window[0] {
                    self.next_char();
                } else {
                    return Err(LexicalError {
                        error: LexicalErrorType::LineContinuationError,
                        location: self.get_pos(),
                    });
                }

                if self.window[0].is_none() {
                    return Err(LexicalError {
                        error: LexicalErrorType::Eof,
                        location: self.get_pos(),
                    });
                }
            }

            _ => {
                let c = self.next_char();
                return Err(LexicalError {
                    error: LexicalErrorType::UnrecognizedToken { tok: c.unwrap() },
                    location: self.get_pos(),
                });
            } // Ignore all the rest..
        }

        Ok(())
    }

    fn eat_single_char(&mut self, ty: Tok) {
        let tok_start = self.get_pos();
        self.next_char().unwrap();
        let tok_end = self.get_pos();
        self.emit((tok_start, ty, tok_end));
    }

    /// Helper function to go to the next character coming up.
    fn next_char(&mut self) -> Option<char> {
        let c = self.window[0];
        self.window.slide();
        if c == Some('\n') {
            self.location.newline();
        } else {
            self.location.go_right();
        }
        c
    }

    /// Helper function to retrieve the current position.
    fn get_pos(&self) -> Location {
        self.location
    }

    /// Helper function to emit a lexed token to the queue of tokens.
    fn emit(&mut self, spanned: Spanned) {
        self.pending.push(spanned);
    }
}

/* Implement iterator pattern for the get_tok function.

Calling the next element in the iterator will yield the next lexical
token.
*/
impl<T> Iterator for Lexer<T>
where
    T: Iterator<Item = char>,
{
    type Item = LexResult;

    fn next(&mut self) -> Option<Self::Item> {
        // Idea: create some sort of hash map for single char tokens:
        // let mut X = HashMap::new();
        // X.insert('=', Tok::Equal);
        let token = self.inner_next();
        trace!(
            "Lex token {:?}, nesting={:?}, indent stack: {:?}",
            token,
            self.nesting,
            self.indentations,
        );

        match token {
            Ok((_, Tok::EndOfFile, _)) => None,
            r => Some(r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{make_tokenizer, NewlineHandler, StringKind, Tok};
    use num_bigint::BigInt;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    pub fn lex_source(source: &str) -> Vec<Tok> {
        let lexer = make_tokenizer(source);
        lexer.map(|x| x.unwrap().1).collect()
    }

    #[test]
    fn test_newline_processor() {
        // Escape \ followed by \n (by removal):
        let src = "b\\\r\n";
        assert_eq!(4, src.len());
        let nlh = NewlineHandler::new(src.chars());
        let x: Vec<char> = nlh.collect();
        assert_eq!(vec!['b', '\\', '\n'], x);
    }

    fn stok(s: &str) -> Tok {
        Tok::String {
            value: s.to_owned(),
            kind: StringKind::Normal,
        }
    }

    #[test]
    fn test_raw_string() {
        let source = "r\"\\\\\" \"\\\\\"";
        let tokens = lex_source(source);
        assert_eq!(tokens, vec![stok("\\\\"), stok("\\"), Tok::Newline,]);
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
            fn $name() {
                let source = format!(r"99232  # {}", $eol);
                let tokens = lex_source(&source);
                assert_eq!(tokens, vec![Tok::Int { value: BigInt::from(99232) }, Tok::Comment, Tok::Newline]);
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
                        Tok::Comment,
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
        let source = r"avariable = 99 + 2-0";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Name {
                    name: String::from("avariable"),
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
                        Tok::Indent,
                        Tok::Return,
                        Tok::Int { value: BigInt::from(99) },
                        Tok::Newline,
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
                        Tok::Indent,
                        Tok::Return,
                        Tok::Int { value: BigInt::from(99) },
                        Tok::Newline,
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
                let source = format!("x = [{}    1,2{}]{}", $eol, $eol, $eol);
                let tokens = lex_source(&source);
                assert_eq!(
                    tokens,
                    vec![
                        Tok::Name {
                            name: String::from("x"),
                        },
                        Tok::Equal,
                        Tok::Lsqb,
                        Tok::Int { value: BigInt::from(1) },
                        Tok::Comma,
                        Tok::Int { value: BigInt::from(2) },
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
                stok("double"),
                stok("single"),
                stok("can't"),
                stok("\\\""),
                stok("\t\r\n"),
                stok("\\g"),
                stok("raw\\'"),
                stok("Đ"),
                stok("\u{80}\u{0}a"),
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
                        stok("abcdef"),
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
    fn test_single_quoted_byte() {
        // single quote
        let source = r##"b'\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff'"##;
        let tokens = lex_source(source);
        let res = (0..=255).collect::<Vec<u8>>();
        assert_eq!(tokens, vec![Tok::Bytes { value: res }, Tok::Newline]);
    }

    #[test]
    fn test_double_quoted_byte() {
        // double quote
        let source = r##"b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff""##;
        let tokens = lex_source(source);
        let res = (0..=255).collect::<Vec<u8>>();
        assert_eq!(tokens, vec![Tok::Bytes { value: res }, Tok::Newline]);
    }

    #[test]
    fn test_escape_char_in_byte_literal() {
        // backslash does not escape
        let source = r##"b"omkmok\Xaa""##;
        let tokens = lex_source(source);
        let res = vec![111, 109, 107, 109, 111, 107, 92, 88, 97, 97];
        assert_eq!(tokens, vec![Tok::Bytes { value: res }, Tok::Newline]);
    }

    #[test]
    fn test_raw_byte_literal() {
        let source = r"rb'\x1z'";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Bytes {
                    value: b"\\x1z".to_vec()
                },
                Tok::Newline
            ]
        );
        let source = r"rb'\\'";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Bytes {
                    value: b"\\\\".to_vec()
                },
                Tok::Newline
            ]
        )
    }

    #[test]
    fn test_escape_octet() {
        let source = r##"b'\43a\4\1234'"##;
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Bytes {
                    value: b"#a\x04S4".to_vec()
                },
                Tok::Newline
            ]
        )
    }

    #[test]
    fn test_escape_unicode_name() {
        let source = r#""\N{EN SPACE}""#;
        let tokens = lex_source(source);
        assert_eq!(tokens, vec![stok("\u{2002}"), Tok::Newline])
    }
}
