//! This module takes care of lexing python source text.
//!
//! This means source code is translated into separate tokens.

pub use super::token::{StringKind, Tok};
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
    tabs: u32,
    spaces: u32,
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
    make_tokenizer_located(source, Location::default())
}

pub fn make_tokenizer_located(
    source: &str,
    start_location: Location,
) -> impl Iterator<Item = LexResult> + '_ {
    Lexer::new(source.chars(), start_location)
}

impl<T> Lexer<T>
where
    T: Iterator<Item = char>,
{
    pub fn new(input: T, start: Location) -> Self {
        let mut lxr = Lexer {
            at_begin_of_line: true,
            nesting: 0,
            indentations: Indentations::default(),
            // Usually we have less than 5 tokens pending.
            pending: Vec::with_capacity(5),
            location: start,
            window: CharWindow::new(input),
        };
        lxr.window.slide();
        lxr.window.slide();
        lxr.window.slide();
        // TODO: Handle possible mismatch between BOM and explicit encoding declaration.
        if let Some('\u{feff}') = lxr.window[0] {
            lxr.window.slide();
        }
        lxr
    }

    // Lexer helper functions:
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
            Ok((start_pos, tok.clone(), end_pos))
        } else {
            Ok((start_pos, Tok::Name { name }, end_pos))
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
    fn lex_number_radix(&mut self, start_pos: Location, radix: u32) -> LexResult {
        let value_text = self.radix_run(radix);
        let end_pos = self.get_pos();
        let value = BigInt::from_str_radix(&value_text, radix).map_err(|e| LexicalError {
            error: LexicalErrorType::OtherError(format!("{e:?}")),
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
        match self.window[..2] {
            [Some('e' | 'E'), Some('+' | '-')] => matches!(self.window[2], Some('0'..='9')),
            [Some('e' | 'E'), Some('0'..='9')] => true,
            _ => false,
        }
    }

    /// Skip everything until end of line
    fn lex_comment(&mut self) -> LexResult {
        let start_pos = self.get_pos();
        let mut value = String::new();
        loop {
            match self.window[0] {
                Some('\n' | '\r') | None => {
                    let end_pos = self.get_pos();
                    return Ok((start_pos, Tok::Comment(value), end_pos));
                }
                Some(_) => {}
            }
            value.push(self.next_char().unwrap());
        }
    }

    fn lex_string(&mut self, kind: StringKind) -> LexResult {
        let start_pos = self.get_pos();
        for _ in 0..kind.prefix_len() {
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
        Ok((start_pos, tok, end_pos))
    }

    fn is_identifier_start(&self, c: char) -> bool {
        match c {
            'a'..='z' | 'A'..='Z' | '_' => true,
            _ => is_xid_start(c),
        }
    }

    fn is_identifier_continuation(&self) -> bool {
        match self.window[0] {
            Some('a'..='z' | 'A'..='Z' | '_' | '0'..='9') => true,
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
                Some('\n' | '\r') => {
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
                        self.emit((tok_start, Tok::Ellipsis, tok_end));
                    } else {
                        let tok_end = self.get_pos();
                        self.emit((tok_start, Tok::Dot, tok_end));
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
                    self.emit((tok_start, Tok::Newline, tok_end));
                } else {
                    self.emit((tok_start, Tok::NonLogicalNewline, tok_end));
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
                match self.window[0] {
                    Some('\n' | '\r') => {
                        self.next_char();
                    }
                    _ => {
                        return Err(LexicalError {
                            error: LexicalErrorType::LineContinuationError,
                            location: self.get_pos(),
                        })
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
                        tok_start,
                        Tok::Name {
                            name: c.to_string(),
                        },
                        tok_end,
                    ));
                } else {
                    let c = self.next_char();
                    return Err(LexicalError {
                        error: LexicalErrorType::UnrecognizedToken { tok: c.unwrap() },
                        location: self.get_pos(),
                    });
                }
            } // Ignore all the rest..
        }

        Ok(())
    }

    fn eat_single_char(&mut self, ty: Tok) {
        let tok_start = self.get_pos();
        self.next_char().unwrap_or_else(|| unsafe {
            // SAFETY: eat_single_char has been called only after a character has been read
            // from the window, so the window is guaranteed to be non-empty.
            std::hint::unreachable_unchecked()
        });
        let tok_end = self.get_pos();
        self.emit((tok_start, ty, tok_end));
    }

    /// Helper function to go to the next character coming up.
    fn next_char(&mut self) -> Option<char> {
        let mut c = self.window[0];
        self.window.slide();
        match c {
            Some('\n') => {
                self.location.newline();
            }
            Some('\r') => {
                if self.window[0] == Some('\n') {
                    self.window.slide();
                }
                self.location.newline();
                c = Some('\n');
            }
            _ => {
                self.location.go_right();
            }
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
    use super::{make_tokenizer, StringKind, Tok};
    use num_bigint::BigInt;

    const WINDOWS_EOL: &str = "\r\n";
    const MAC_EOL: &str = "\r";
    const UNIX_EOL: &str = "\n";

    pub fn lex_source(source: &str) -> Vec<Tok> {
        let lexer = make_tokenizer(source);
        lexer.map(|x| x.unwrap().1).collect()
    }

    fn stok(s: &str) -> Tok {
        Tok::String {
            value: s.to_owned(),
            kind: StringKind::String,
            triple_quoted: false,
        }
    }

    fn raw_stok(s: &str) -> Tok {
        Tok::String {
            value: s.to_owned(),
            kind: StringKind::RawString,
            triple_quoted: false,
        }
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
                stok("a"),
                Tok::NonLogicalNewline,
                stok("b"),
                Tok::NonLogicalNewline,
                Tok::NonLogicalNewline,
                stok("c"),
                stok("d"),
                Tok::NonLogicalNewline,
                Tok::Rpar,
                Tok::Newline,
            ]
        );
    }

    #[test]
    fn test_logical_newline_line_comment() {
        let source = "#Hello\n#World";
        let tokens = lex_source(source);
        assert_eq!(
            tokens,
            vec![
                Tok::Comment("#Hello".to_owned()),
                // tokenize.py does put an NL here...
                Tok::Comment("#World".to_owned()),
                // ... and here, but doesn't seem very useful.
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
                stok("double"),
                stok("single"),
                stok(r"can\'t"),
                stok(r#"\\\""#),
                stok(r"\t\r\n"),
                stok(r"\g"),
                raw_stok(r"raw\'"),
                stok(r"\420"),
                stok(r"\200\0a"),
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
                        stok("abc\\\ndef"),
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
        assert_eq!(tokens, vec![stok(r"\N{EN SPACE}"), Tok::Newline])
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
