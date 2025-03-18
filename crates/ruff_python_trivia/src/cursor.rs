use std::str::Chars;

use ruff_text_size::{TextLen, TextSize};

pub const EOF_CHAR: char = '\0';

/// A [`Cursor`] over a string.
///
/// Based on [`rustc`'s `Cursor`](https://github.com/rust-lang/rust/blob/d1b7355d3d7b4ead564dbecb1d240fcc74fff21b/compiler/rustc_lexer/src/cursor.rs)
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    chars: Chars<'a>,
    source_length: TextSize,
}

impl<'a> Cursor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source_length: source.text_len(),
            chars: source.chars(),
        }
    }

    /// Return the remaining input as a string slice.
    pub fn chars(&self) -> Chars<'a> {
        self.chars.clone()
    }

    /// Returns the remaining input as byte slice.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.as_str().as_bytes()
    }

    /// Returns the remaining input as string slice.
    pub fn as_str(&self) -> &'a str {
        self.chars.as_str()
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the file is at the end of the file.
    pub fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Peeks the second character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file.
    pub fn second(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the file is at the end of the file.
    pub fn last(&self) -> char {
        self.chars.clone().next_back().unwrap_or(EOF_CHAR)
    }

    pub fn text_len(&self) -> TextSize {
        self.chars.as_str().text_len()
    }

    pub fn token_len(&self) -> TextSize {
        self.source_length - self.text_len()
    }

    pub fn start_token(&mut self) {
        self.source_length = self.text_len();
    }

    /// Returns `true` if the file is at the end of the file.
    pub fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Consumes the next character
    pub fn bump(&mut self) -> Option<char> {
        self.chars.next()
    }

    /// Consumes the next character from the back
    pub fn bump_back(&mut self) -> Option<char> {
        self.chars.next_back()
    }

    pub fn eat_char(&mut self, c: char) -> bool {
        if self.first() == c {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Eats the next two characters if they are `c1` and `c2`. Does not
    /// consume any input otherwise, even if the first character matches.
    pub fn eat_char2(&mut self, c1: char, c2: char) -> bool {
        let mut chars = self.chars.clone();
        if chars.next() == Some(c1) && chars.next() == Some(c2) {
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    /// Eats the next three characters if they are `c1`, `c2` and `c3`
    /// Does not consume any input otherwise, even if the first character matches.
    pub fn eat_char3(&mut self, c1: char, c2: char, c3: char) -> bool {
        let mut chars = self.chars.clone();
        if chars.next() == Some(c1) && chars.next() == Some(c2) && chars.next() == Some(c3) {
            self.bump();
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn eat_char_back(&mut self, c: char) -> bool {
        if self.last() == c {
            self.bump_back();
            true
        } else {
            false
        }
    }

    /// Eats the next character if `predicate` returns `true`.
    pub fn eat_if(&mut self, mut predicate: impl FnMut(char) -> bool) -> bool {
        if predicate(self.first()) && !self.is_eof() {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    pub fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        // It was tried making optimized version of this for eg. line comments, but
        // LLVM can inline all of this and compile it down to fast iteration over bytes.
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }

    /// Eats symbols from the back while predicate returns true or until the beginning of file is reached.
    pub fn eat_back_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        // It was tried making optimized version of this for eg. line comments, but
        // LLVM can inline all of this and compile it down to fast iteration over bytes.
        while predicate(self.last()) && !self.is_eof() {
            self.bump_back();
        }
    }

    /// Skips the next `count` bytes.
    ///
    /// ## Panics
    ///  - If `count` is larger than the remaining bytes in the input stream.
    ///  - If `count` indexes into a multi-byte character.
    pub fn skip_bytes(&mut self, count: usize) {
        self.chars = self.chars.as_str()[count..].chars();
    }
}
