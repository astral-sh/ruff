use ruff_text_size::{TextLen, TextSize};
use std::str::Chars;

pub(crate) const EOF_CHAR: char = '\0';

#[derive(Clone, Debug)]
pub(super) struct Cursor<'a> {
    chars: Chars<'a>,
    source_length: TextSize,
    #[cfg(debug_assertions)]
    prev_char: char,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source_length: source.text_len(),
            chars: source.chars(),
            #[cfg(debug_assertions)]
            prev_char: EOF_CHAR,
        }
    }

    /// Returns the previous token. Useful for debug assertions.
    #[cfg(debug_assertions)]
    pub(super) const fn previous(&self) -> char {
        self.prev_char
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the file is at the end of the file.
    pub(super) fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Peeks the second character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file.
    pub(super) fn second(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Returns the remaining text to lex.
    pub(super) fn rest(&self) -> &'a str {
        self.chars.as_str()
    }

    // SAFETY: The `source.text_len` call in `new` would panic if the string length is larger than a `u32`.
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn text_len(&self) -> TextSize {
        TextSize::new(self.chars.as_str().len() as u32)
    }

    pub(super) fn token_len(&self) -> TextSize {
        self.source_length - self.text_len()
    }

    pub(super) fn start_token(&mut self) {
        self.source_length = self.text_len();
    }

    pub(super) fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Consumes the next character
    pub(super) fn bump(&mut self) -> Option<char> {
        let prev = self.chars.next()?;

        #[cfg(debug_assertions)]
        {
            self.prev_char = prev;
        }

        Some(prev)
    }

    pub(super) fn eat_char(&mut self, c: char) -> bool {
        if self.first() == c {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn eat_char2(&mut self, c1: char, c2: char) -> bool {
        let mut chars = self.chars.clone();
        if chars.next() == Some(c1) && chars.next() == Some(c2) {
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn eat_if<F>(&mut self, mut predicate: F) -> Option<char>
    where
        F: FnMut(char) -> bool,
    {
        if predicate(self.first()) && !self.is_eof() {
            self.bump()
        } else {
            None
        }
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    pub(super) fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        // It was tried making optimized version of this for eg. line comments, but
        // LLVM can inline all of this and compile it down to fast iteration over bytes.
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }
}
