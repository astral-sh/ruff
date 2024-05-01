use ruff_text_size::{TextLen, TextSize};

pub(crate) const EOF_CHAR: char = '\0';

#[derive(Clone, Debug)]
pub(super) struct Cursor<'src> {
    /// Source text.
    source: &'src str,

    /// The start position in the source text of the next character.
    position: usize,

    /// Offset of the current token from the start of the source. The length of the current
    /// token can be computed by `self.position - self.current_start`.
    current_start: TextSize,

    #[cfg(debug_assertions)]
    prev_char: char,
}

impl<'src> Cursor<'src> {
    pub(crate) fn new(source: &'src str) -> Self {
        assert!(TextSize::try_from(source.len()).is_ok());

        Self {
            source,
            position: 0,
            current_start: TextSize::new(0),
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
        self.rest().chars().next().unwrap_or(EOF_CHAR)
    }

    /// Peeks the second character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file.
    pub(super) fn second(&self) -> char {
        let mut chars = self.rest().chars();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Returns the remaining text to lex.
    pub(super) fn rest(&self) -> &'src str {
        self.source.get(self.position..).unwrap_or("")
    }

    // SAFETY: The `Cursor::new` call would panic if the string length is larger than a `u32`.
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn text_len(&self) -> TextSize {
        TextSize::new(self.rest().len() as u32)
    }

    // SAFETY: The `Cursor::new` call would panic if the string length is larger than a `u32`.
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn token_len(&self) -> TextSize {
        TextSize::new(self.position as u32) - self.current_start
    }

    // SAFETY: The `Cursor::new` call would panic if the string length is larger than a `u32`.
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn start_token(&mut self) {
        self.current_start = TextSize::new(self.position as u32);
    }

    pub(super) fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }

    /// Consumes the next character
    pub(super) fn bump(&mut self) -> Option<char> {
        let prev = self.rest().chars().next()?;
        self.position += prev.text_len().to_usize();

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
        let mut chars = self.rest().chars();
        if chars.next() == Some(c1) && chars.next() == Some(c2) {
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn eat_char3(&mut self, c1: char, c2: char, c3: char) -> bool {
        let mut chars = self.rest().chars();
        if chars.next() == Some(c1) && chars.next() == Some(c2) && chars.next() == Some(c3) {
            self.bump();
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
    #[inline]
    pub(super) fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        // It was tried making optimized version of this for eg. line comments, but
        // LLVM can inline all of this and compile it down to fast iteration over bytes.
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }

    /// Skips the next `count` bytes.
    ///
    /// ## Panics
    ///  - If `count` is larger than the remaining bytes in the input stream.
    ///  - If `count` indexes into a multi-byte character.
    pub(super) fn skip_bytes(&mut self, count: usize) {
        #[cfg(debug_assertions)]
        {
            self.prev_char = self.rest()[..count].chars().next_back().unwrap_or('\0');
        }

        self.position += count;
    }

    /// Skips to the end of the input stream.
    pub(super) fn skip_to_end(&mut self) {
        self.position = self.source.len();
    }
}
