use std::str::Chars;

use ruff_text_size::{TextLen, TextSize};

pub(crate) const EOF_CHAR: char = '\0';

/// A cursor represents a pointer in the source code.
///
/// Based on [`rustc`'s `Cursor`](https://github.com/rust-lang/rust/blob/d1b7355d3d7b4ead564dbecb1d240fcc74fff21b/compiler/rustc_lexer/src/cursor.rs)
#[derive(Clone, Debug)]
pub(super) struct Cursor<'src> {
    /// An iterator over the [`char`]'s of the source code.
    chars: Chars<'src>,

    /// Length of the source code. This is used as a marker to indicate the start of the current
    /// token which is being lexed.
    source_length: TextSize,

    /// Total length of the original source. Used to compute absolute cursor position
    /// for cell boundary checks.
    total_source_length: TextSize,

    /// Stores the previous character for debug assertions.
    #[cfg(debug_assertions)]
    prev_char: char,

    /// Cell offsets for Jupyter notebook sources. Tracks the byte offsets where each cell
    /// begins in the concatenated source. Empty slice for non-notebook sources.
    cell_offsets: &'src [TextSize],

    /// Index into `cell_offsets` indicating the current cell the cursor is in.
    current_cell: usize,

    /// Whether cell offsets are set. Used to skip cell boundary checks on the hot path
    /// for non-notebook sources, avoiding function call overhead on every token.
    has_cells: bool,
}

impl<'src> Cursor<'src> {
    pub(crate) fn new(source: &'src str) -> Self {
        Self {
            source_length: source.text_len(),
            total_source_length: source.text_len(),
            chars: source.chars(),
            #[cfg(debug_assertions)]
            prev_char: EOF_CHAR,
            cell_offsets: &[],
            current_cell: 0,
            has_cells: false,
        }
    }

    /// Set cell offsets for notebook cell boundary awareness.
    pub(crate) fn set_cell_offsets(&mut self, cell_offsets: &'src [TextSize]) {
        self.has_cells = !cell_offsets.is_empty();
        self.cell_offsets = cell_offsets;
    }

    /// Returns the current cell index.
    pub(crate) fn current_cell(&self) -> usize {
        self.current_cell
    }

    /// Set the current cell index (used during checkpoint/rewind).
    pub(crate) fn set_current_cell(&mut self, cell: usize) {
        self.current_cell = cell;
    }

    /// Returns `true` if the cursor is positioned at a notebook cell boundary.
    ///
    /// A cell boundary occurs when the cursor position equals the start offset
    /// of the next cell (i.e., `cell_offsets[current_cell + 1]`).
    pub(crate) fn is_at_cell_boundary(&self) -> bool {
        if self.cell_offsets.is_empty() {
            return false;
        }

        // current_cell tracks which cell we're in. The next cell starts at
        // cell_offsets[current_cell + 1]. If cursor position equals that offset,
        // we're at a boundary.
        let Some(&next_cell_start) = self.cell_offsets.get(self.current_cell + 1) else {
            return false;
        };

        // Compute current offset: total source length minus remaining chars
        let current_offset = self.total_source_length - self.text_len();
        current_offset == next_cell_start
    }

    /// Advance past the current cell boundary. Returns `true` if there are more cells,
    /// `false` if this was the last cell.
    pub(crate) fn next_cell(&mut self) -> bool {
        if self.current_cell + 1 < self.cell_offsets.len() {
            self.current_cell += 1;
            true
        } else {
            false
        }
    }

    /// Returns the previous character. Useful for debug assertions.
    #[cfg(debug_assertions)]
    pub(super) const fn previous(&self) -> char {
        self.prev_char
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file or at a cell boundary.
    pub(super) fn first(&self) -> char {
        if self.has_cells && self.is_at_cell_boundary() {
            return EOF_CHAR;
        }
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
    ///
    /// Use [`Cursor::text_len`] to get the length of the remaining text.
    pub(super) fn rest(&self) -> &'src str {
        self.chars.as_str()
    }

    /// Returns the length of the remaining text.
    ///
    /// Use [`Cursor::rest`] to get the remaining text.
    // SAFETY: The `source.text_len` call in `new` would panic if the string length is larger than a `u32`.
    #[expect(clippy::cast_possible_truncation)]
    pub(super) fn text_len(&self) -> TextSize {
        TextSize::new(self.chars.as_str().len() as u32)
    }

    /// Returns the length of the current token length.
    ///
    /// This is to be used after setting the start position of the token using
    /// [`Cursor::start_token`].
    pub(super) fn token_len(&self) -> TextSize {
        self.source_length - self.text_len()
    }

    /// Mark the current position of the cursor as the start of the token which is going to be
    /// lexed.
    ///
    /// Use [`Cursor::token_len`] to get the length of the lexed token.
    pub(super) fn start_token(&mut self) {
        self.source_length = self.text_len();
    }

    /// Returns `true` if the cursor is at the end of file or at a cell boundary.
    pub(super) fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty() || (self.has_cells && self.is_at_cell_boundary())
    }

    /// Moves the cursor to the next character, returning the previous character.
    /// Returns [`None`] if there is no next character or at a cell boundary.
    pub(super) fn bump(&mut self) -> Option<char> {
        if self.has_cells && self.is_at_cell_boundary() {
            return None;
        }

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

    pub(super) fn eat_char3(&mut self, c1: char, c2: char, c3: char) -> bool {
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
            self.prev_char = self.chars.as_str()[..count]
                .chars()
                .next_back()
                .unwrap_or('\0');
        }

        self.chars = self.chars.as_str()[count..].chars();
    }

    /// Skips to the end of the input stream.
    pub(super) fn skip_to_end(&mut self) {
        self.chars = "".chars();
    }
}
