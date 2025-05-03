use std::fmt;
use std::fmt::{Debug, Formatter};
use std::num::{NonZeroUsize, ParseIntError};
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use crate::{LineColumn, SourceLocation};
use ruff_text_size::{TextLen, TextRange, TextSize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Index for fast [byte offset](TextSize) to [`LineColumn`] conversions.
///
/// Cloning a [`LineIndex`] is cheap because it only requires bumping a reference count.
#[derive(Clone, Eq, PartialEq)]
pub struct LineIndex {
    inner: Arc<LineIndexInner>,
}

#[derive(Eq, PartialEq)]
struct LineIndexInner {
    line_starts: Vec<TextSize>,
    kind: IndexKind,
}

impl LineIndex {
    /// Builds the [`LineIndex`] from the source text of a file.
    pub fn from_source_text(text: &str) -> Self {
        let mut line_starts: Vec<TextSize> = Vec::with_capacity(text.len() / 88);
        line_starts.push(TextSize::default());

        let bytes = text.as_bytes();
        let mut utf8 = false;

        assert!(u32::try_from(bytes.len()).is_ok());

        for (i, byte) in bytes.iter().enumerate() {
            utf8 |= !byte.is_ascii();

            match byte {
                // Only track one line break for `\r\n`.
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => continue,
                b'\n' | b'\r' => {
                    // SAFETY: Assertion above guarantees `i <= u32::MAX`
                    #[expect(clippy::cast_possible_truncation)]
                    line_starts.push(TextSize::from(i as u32) + TextSize::from(1));
                }
                _ => {}
            }
        }

        let kind = if utf8 {
            IndexKind::Utf8
        } else {
            IndexKind::Ascii
        };

        Self {
            inner: Arc::new(LineIndexInner { line_starts, kind }),
        }
    }

    fn kind(&self) -> IndexKind {
        self.inner.kind
    }

    /// Returns the line and column number for an UTF-8 byte offset.
    ///
    /// The `column` number is the nth-character of the line, except for the first line
    /// where it doesn't include the UTF-8 BOM marker at the start of the file.
    ///
    /// ### BOM handling
    ///
    /// For files starting with a UTF-8 BOM marker, the byte offsets
    /// in the range `0...3` are all mapped to line 0 and column 0.
    /// Because of this, the conversion isn't losless.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::TextSize;
    /// # use ruff_source_file::{LineIndex, OneIndexed, LineColumn};
    /// let source = format!("\u{FEFF}{}", "def a():\n    pass");
    /// let index = LineIndex::from_source_text(&source);
    ///
    /// // Before BOM, maps to after BOM
    /// assert_eq!(
    ///     index.line_column(TextSize::from(0), &source),
    ///     LineColumn { line: OneIndexed::from_zero_indexed(0), column: OneIndexed::from_zero_indexed(0) }
    /// );
    ///
    /// // After BOM, maps to after BOM
    /// assert_eq!(
    ///     index.line_column(TextSize::from(3), &source),
    ///     LineColumn { line: OneIndexed::from_zero_indexed(0), column: OneIndexed::from_zero_indexed(0) }
    /// );
    ///
    /// assert_eq!(
    ///     index.line_column(TextSize::from(7), &source),
    ///     LineColumn { line: OneIndexed::from_zero_indexed(0), column: OneIndexed::from_zero_indexed(4) }
    /// );
    /// assert_eq!(
    ///     index.line_column(TextSize::from(16), &source),
    ///     LineColumn { line: OneIndexed::from_zero_indexed(1), column: OneIndexed::from_zero_indexed(4) }
    /// );
    /// ```
    ///
    /// ## Panics
    ///
    /// If the byte offset isn't within the bounds of `content`.
    pub fn line_column(&self, offset: TextSize, content: &str) -> LineColumn {
        let location = self.source_location(offset, content, PositionEncoding::Utf32);

        // Don't count the BOM character as a column, but only on the first line.
        let column = if location.line.to_zero_indexed() == 0 && content.starts_with('\u{feff}') {
            location.character_offset.saturating_sub(1)
        } else {
            location.character_offset
        };

        LineColumn {
            line: location.line,
            column,
        }
    }

    /// Given a UTF-8 byte offset, returns the line and character offset according to the given encoding.
    ///
    /// ### BOM handling
    ///
    /// Unlike [`Self::line_column`], this method does not skip the BOM character at the start of the file.
    /// This allows for bidirectional mapping between [`SourceLocation`] and [`TextSize`] (see [`Self::offset`]).
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::TextSize;
    /// # use ruff_source_file::{LineIndex, OneIndexed, LineColumn, SourceLocation, PositionEncoding, Line};
    /// let source = format!("\u{FEFF}{}", "def a():\n    pass");
    /// let index = LineIndex::from_source_text(&source);
    ///
    /// // Before BOM, maps to character 0
    /// assert_eq!(
    ///     index.source_location(TextSize::from(0), &source, PositionEncoding::Utf32),
    ///     SourceLocation { line: OneIndexed::from_zero_indexed(0), character_offset: OneIndexed::from_zero_indexed(0) }
    /// );
    ///
    /// // After BOM, maps to after BOM
    /// assert_eq!(
    ///     index.source_location(TextSize::from(3), &source, PositionEncoding::Utf32),
    ///     SourceLocation { line: OneIndexed::from_zero_indexed(0), character_offset: OneIndexed::from_zero_indexed(1) }
    /// );
    ///
    /// assert_eq!(
    ///     index.source_location(TextSize::from(7), &source, PositionEncoding::Utf32),
    ///     SourceLocation { line: OneIndexed::from_zero_indexed(0), character_offset: OneIndexed::from_zero_indexed(5) }
    /// );
    /// assert_eq!(
    ///     index.source_location(TextSize::from(16), &source, PositionEncoding::Utf32),
    ///     SourceLocation { line: OneIndexed::from_zero_indexed(1), character_offset: OneIndexed::from_zero_indexed(4) }
    /// );
    /// ```
    ///
    /// ## Panics
    ///
    /// If the UTF-8 byte offset is out of bounds of `text`.
    pub fn source_location(
        &self,
        offset: TextSize,
        text: &str,
        encoding: PositionEncoding,
    ) -> SourceLocation {
        let line = self.line_index(offset);
        let line_start = self.line_start(line, text);

        if self.is_ascii() {
            return SourceLocation {
                line,
                character_offset: OneIndexed::from_zero_indexed((offset - line_start).to_usize()),
            };
        }

        match encoding {
            PositionEncoding::Utf8 => {
                let character_offset = offset - line_start;
                SourceLocation {
                    line,
                    character_offset: OneIndexed::from_zero_indexed(character_offset.to_usize()),
                }
            }
            PositionEncoding::Utf16 => {
                let up_to_character = &text[TextRange::new(line_start, offset)];
                let character = up_to_character.encode_utf16().count();

                SourceLocation {
                    line,
                    character_offset: OneIndexed::from_zero_indexed(character),
                }
            }
            PositionEncoding::Utf32 => {
                let up_to_character = &text[TextRange::new(line_start, offset)];
                let character = up_to_character.chars().count();

                SourceLocation {
                    line,
                    character_offset: OneIndexed::from_zero_indexed(character),
                }
            }
        }
    }

    /// Return the number of lines in the source code.
    pub fn line_count(&self) -> usize {
        self.line_starts().len()
    }

    /// Returns `true` if the text only consists of ASCII characters
    pub fn is_ascii(&self) -> bool {
        self.kind().is_ascii()
    }

    /// Returns the row number for a given offset.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::TextSize;
    /// # use ruff_source_file::{LineIndex, OneIndexed, LineColumn};
    /// let source = "def a():\n    pass";
    /// let index = LineIndex::from_source_text(source);
    ///
    /// assert_eq!(index.line_index(TextSize::from(0)), OneIndexed::from_zero_indexed(0));
    /// assert_eq!(index.line_index(TextSize::from(4)), OneIndexed::from_zero_indexed(0));
    /// assert_eq!(index.line_index(TextSize::from(13)), OneIndexed::from_zero_indexed(1));
    /// ```
    ///
    /// ## Panics
    ///
    /// If the offset is out of bounds.
    pub fn line_index(&self, offset: TextSize) -> OneIndexed {
        match self.line_starts().binary_search(&offset) {
            // Offset is at the start of a line
            Ok(row) => OneIndexed::from_zero_indexed(row),
            Err(row) => {
                // SAFETY: Safe because the index always contains an entry for the offset 0
                OneIndexed::from_zero_indexed(row - 1)
            }
        }
    }

    /// Returns the [byte offset](TextSize) for the `line` with the given index.
    pub fn line_start(&self, line: OneIndexed, contents: &str) -> TextSize {
        let row_index = line.to_zero_indexed();
        let starts = self.line_starts();

        // If start-of-line position after last line
        if row_index == starts.len() {
            contents.text_len()
        } else {
            starts[row_index]
        }
    }

    /// Returns the [byte offset](TextSize) of the `line`'s end.
    /// The offset is the end of the line, up to and including the newline character ending the line (if any).
    pub fn line_end(&self, line: OneIndexed, contents: &str) -> TextSize {
        let row_index = line.to_zero_indexed();
        let starts = self.line_starts();

        // If start-of-line position after last line
        if row_index.saturating_add(1) >= starts.len() {
            contents.text_len()
        } else {
            starts[row_index + 1]
        }
    }

    /// Returns the [byte offset](TextSize) of the `line`'s end.
    /// The offset is the end of the line, excluding the newline character ending the line (if any).
    pub fn line_end_exclusive(&self, line: OneIndexed, contents: &str) -> TextSize {
        let row_index = line.to_zero_indexed();
        let starts = self.line_starts();

        // If start-of-line position after last line
        if row_index.saturating_add(1) >= starts.len() {
            contents.text_len()
        } else {
            starts[row_index + 1] - TextSize::new(1)
        }
    }

    /// Returns the [`TextRange`] of the `line` with the given index.
    /// The start points to the first character's [byte offset](TextSize), the end up to, and including
    /// the newline character ending the line (if any).
    pub fn line_range(&self, line: OneIndexed, contents: &str) -> TextRange {
        let starts = self.line_starts();

        if starts.len() == line.to_zero_indexed() {
            TextRange::empty(contents.text_len())
        } else {
            TextRange::new(
                self.line_start(line, contents),
                self.line_start(line.saturating_add(1), contents),
            )
        }
    }

    /// Returns the [UTF-8 byte offset](TextSize) at `line` and `character` where character is counted using the given encoding.
    ///
    /// ## Examples
    ///
    /// ### ASCII only source text
    ///
    /// ```
    /// # use ruff_source_file::{SourceLocation, LineIndex, OneIndexed, PositionEncoding};
    /// # use ruff_text_size::TextSize;
    /// let source = r#"a = 4
    /// c = "some string"
    /// x = b"#;
    ///
    /// let index = LineIndex::from_source_text(source);
    ///
    /// // First line, first character
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(0),
    ///             character_offset: OneIndexed::from_zero_indexed(0)
    ///         },
    ///         source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(0)
    ///  );
    ///
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(1),
    ///             character_offset: OneIndexed::from_zero_indexed(4)
    ///         },
    ///         source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(10)
    ///  );
    ///
    /// // Offset past the end of the first line
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(0),
    ///             character_offset: OneIndexed::from_zero_indexed(10)
    ///         },
    ///         source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(6)
    ///  );
    ///
    /// // Offset past the end of the file
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(3),
    ///             character_offset: OneIndexed::from_zero_indexed(0)
    ///         },
    ///         source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(29)
    ///  );
    /// ```
    ///
    /// ### Non-ASCII source text
    ///
    /// ```
    /// use ruff_source_file::{LineIndex, OneIndexed, SourceLocation, PositionEncoding};
    /// use ruff_text_size::TextSize;
    /// let source = format!("\u{FEFF}{}", r#"a = 4
    /// c = "❤️"
    /// x = b"#);
    ///
    /// let index = LineIndex::from_source_text(&source);
    ///
    /// // First line, first character, points at the BOM
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(0),
    ///             character_offset: OneIndexed::from_zero_indexed(0)
    ///         },
    ///         &source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(0)
    ///  );
    ///
    /// // First line, after the BOM
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(0),
    ///             character_offset: OneIndexed::from_zero_indexed(1)
    ///         },
    ///         &source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(3)
    ///  );
    ///
    /// // second line, 7th character, after emoji, UTF32
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(1),
    ///             character_offset: OneIndexed::from_zero_indexed(7)
    ///         },
    ///         &source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(20)
    ///  );
    ///
    /// // Second line, 7th character, after emoji, UTF 16
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(1),
    ///             character_offset: OneIndexed::from_zero_indexed(7)
    ///         },
    ///         &source,
    ///         PositionEncoding::Utf16,
    ///     ),
    ///     TextSize::new(20)
    ///  );
    ///
    ///
    /// // Offset past the end of the second line
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(1),
    ///             character_offset: OneIndexed::from_zero_indexed(10)
    ///         },
    ///         &source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(22)
    ///  );
    ///
    /// // Offset past the end of the file
    /// assert_eq!(
    ///     index.offset(
    ///         SourceLocation {
    ///             line: OneIndexed::from_zero_indexed(3),
    ///             character_offset: OneIndexed::from_zero_indexed(0)
    ///         },
    ///         &source,
    ///         PositionEncoding::Utf32,
    ///     ),
    ///     TextSize::new(27)
    ///  );
    /// ```
    pub fn offset(
        &self,
        position: SourceLocation,
        text: &str,
        position_encoding: PositionEncoding,
    ) -> TextSize {
        // If start-of-line position after last line
        if position.line.to_zero_indexed() > self.line_starts().len() {
            return text.text_len();
        }

        let line_range = self.line_range(position.line, text);

        let character_offset = position.character_offset.to_zero_indexed();
        let character_byte_offset = if self.is_ascii() {
            TextSize::try_from(character_offset).unwrap()
        } else {
            let line = &text[line_range];

            match position_encoding {
                PositionEncoding::Utf8 => {
                    TextSize::try_from(position.character_offset.to_zero_indexed()).unwrap()
                }
                PositionEncoding::Utf16 => {
                    let mut byte_offset = TextSize::new(0);
                    let mut utf16_code_unit_offset = 0;

                    for c in line.chars() {
                        if utf16_code_unit_offset >= character_offset {
                            break;
                        }

                        // Count characters encoded as two 16 bit words as 2 characters.
                        byte_offset += c.text_len();
                        utf16_code_unit_offset += c.len_utf16();
                    }

                    byte_offset
                }
                PositionEncoding::Utf32 => line
                    .chars()
                    .take(position.character_offset.to_zero_indexed())
                    .map(ruff_text_size::TextLen::text_len)
                    .sum(),
            }
        };

        line_range.start() + character_byte_offset.clamp(TextSize::new(0), line_range.len())
    }

    /// Returns the [byte offsets](TextSize) for every line
    pub fn line_starts(&self) -> &[TextSize] {
        &self.inner.line_starts
    }
}

impl Deref for LineIndex {
    type Target = [TextSize];

    fn deref(&self) -> &Self::Target {
        self.line_starts()
    }
}

impl Debug for LineIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.line_starts()).finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum IndexKind {
    /// Optimized index for an ASCII only document
    Ascii,

    /// Index for UTF8 documents
    Utf8,
}

impl IndexKind {
    const fn is_ascii(self) -> bool {
        matches!(self, IndexKind::Ascii)
    }
}

/// Type-safe wrapper for a value whose logical range starts at `1`, for
/// instance the line or column numbers in a file
///
/// Internally this is represented as a [`NonZeroUsize`], this enables some
/// memory optimizations
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OneIndexed(NonZeroUsize);

impl OneIndexed {
    /// The largest value that can be represented by this integer type
    pub const MAX: Self = unwrap(Self::new(usize::MAX));
    // SAFETY: These constants are being initialized with non-zero values
    /// The smallest value that can be represented by this integer type.
    pub const MIN: Self = unwrap(Self::new(1));
    pub const ONE: NonZeroUsize = unwrap(NonZeroUsize::new(1));

    /// Creates a non-zero if the given value is not zero.
    pub const fn new(value: usize) -> Option<Self> {
        match NonZeroUsize::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Construct a new [`OneIndexed`] from a zero-indexed value
    pub const fn from_zero_indexed(value: usize) -> Self {
        Self(Self::ONE.saturating_add(value))
    }

    /// Returns the value as a primitive type.
    pub const fn get(self) -> usize {
        self.0.get()
    }

    /// Return the zero-indexed primitive value for this [`OneIndexed`]
    pub const fn to_zero_indexed(self) -> usize {
        self.0.get() - 1
    }

    /// Saturating integer addition. Computes `self + rhs`, saturating at
    /// the numeric bounds instead of overflowing.
    #[must_use]
    pub const fn saturating_add(self, rhs: usize) -> Self {
        match NonZeroUsize::new(self.0.get().saturating_add(rhs)) {
            Some(value) => Self(value),
            None => Self::MAX,
        }
    }

    /// Saturating integer subtraction. Computes `self - rhs`, saturating
    /// at the numeric bounds instead of overflowing.
    #[must_use]
    pub const fn saturating_sub(self, rhs: usize) -> Self {
        match NonZeroUsize::new(self.0.get().saturating_sub(rhs)) {
            Some(value) => Self(value),
            None => Self::MIN,
        }
    }

    /// Checked addition. Returns `None` if overflow occurred.
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0.get()).map(Self)
    }

    /// Checked subtraction. Returns `None` if overflow occurred.
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.get().checked_sub(rhs.get()).and_then(Self::new)
    }
}

impl fmt::Display for OneIndexed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        std::fmt::Debug::fmt(&self.0.get(), f)
    }
}

/// A const `Option::unwrap` without nightly features:
/// [Tracking issue](https://github.com/rust-lang/rust/issues/67441)
const fn unwrap<T: Copy>(option: Option<T>) -> T {
    match option {
        Some(value) => value,
        None => panic!("unwrapping None"),
    }
}

impl FromStr for OneIndexed {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(OneIndexed(NonZeroUsize::from_str(s)?))
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PositionEncoding {
    /// Character offsets count the number of bytes from the start of the line.
    Utf8,

    /// Character offsets count the number of UTF-16 code units from the start of the line.
    Utf16,

    /// Character offsets count the number of UTF-32 code points units (the same as number of characters in Rust)
    /// from the start of the line.
    Utf32,
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextSize;

    use crate::line_index::LineIndex;
    use crate::{LineColumn, OneIndexed};

    #[test]
    fn ascii_index() {
        let index = LineIndex::from_source_text("");
        assert_eq!(index.line_starts(), &[TextSize::from(0)]);

        let index = LineIndex::from_source_text("x = 1");
        assert_eq!(index.line_starts(), &[TextSize::from(0)]);

        let index = LineIndex::from_source_text("x = 1\n");
        assert_eq!(index.line_starts(), &[TextSize::from(0), TextSize::from(6)]);

        let index = LineIndex::from_source_text("x = 1\ny = 2\nz = x + y\n");
        assert_eq!(
            index.line_starts(),
            &[
                TextSize::from(0),
                TextSize::from(6),
                TextSize::from(12),
                TextSize::from(22)
            ]
        );
    }

    #[test]
    fn ascii_source_location() {
        let contents = "x = 1\ny = 2";
        let index = LineIndex::from_source_text(contents);

        // First row.
        let loc = index.line_column(TextSize::from(2), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(2)
            }
        );

        // Second row.
        let loc = index.line_column(TextSize::from(6), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            }
        );

        let loc = index.line_column(TextSize::from(11), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(5)
            }
        );
    }

    #[test]
    fn ascii_carriage_return() {
        let contents = "x = 4\ry = 3";
        let index = LineIndex::from_source_text(contents);
        assert_eq!(index.line_starts(), &[TextSize::from(0), TextSize::from(6)]);

        assert_eq!(
            index.line_column(TextSize::from(4), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(4)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(6), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(7), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(1)
            }
        );
    }

    #[test]
    fn ascii_carriage_return_newline() {
        let contents = "x = 4\r\ny = 3";
        let index = LineIndex::from_source_text(contents);
        assert_eq!(index.line_starts(), &[TextSize::from(0), TextSize::from(7)]);

        assert_eq!(
            index.line_column(TextSize::from(4), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(4)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(7), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(8), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(1)
            }
        );
    }

    #[test]
    fn utf8_index() {
        let index = LineIndex::from_source_text("x = '🫣'");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_starts(), &[TextSize::from(0)]);

        let index = LineIndex::from_source_text("x = '🫣'\n");
        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index.line_starts(),
            &[TextSize::from(0), TextSize::from(11)]
        );

        let index = LineIndex::from_source_text("x = '🫣'\ny = 2\nz = x + y\n");
        assert_eq!(index.line_count(), 4);
        assert_eq!(
            index.line_starts(),
            &[
                TextSize::from(0),
                TextSize::from(11),
                TextSize::from(17),
                TextSize::from(27)
            ]
        );

        let index = LineIndex::from_source_text("# 🫣\nclass Foo:\n    \"\"\".\"\"\"");
        assert_eq!(index.line_count(), 3);
        assert_eq!(
            index.line_starts(),
            &[TextSize::from(0), TextSize::from(7), TextSize::from(18)]
        );
    }

    #[test]
    fn utf8_carriage_return() {
        let contents = "x = '🫣'\ry = 3";
        let index = LineIndex::from_source_text(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index.line_starts(),
            &[TextSize::from(0), TextSize::from(11)]
        );

        // Second '
        assert_eq!(
            index.line_column(TextSize::from(9), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(6)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(11), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(12), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(1)
            }
        );
    }

    #[test]
    fn utf8_carriage_return_newline() {
        let contents = "x = '🫣'\r\ny = 3";
        let index = LineIndex::from_source_text(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index.line_starts(),
            &[TextSize::from(0), TextSize::from(12)]
        );

        // Second '
        assert_eq!(
            index.line_column(TextSize::from(9), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(6)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(12), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            }
        );
        assert_eq!(
            index.line_column(TextSize::from(13), contents),
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(1)
            }
        );
    }

    #[test]
    fn utf8_byte_offset() {
        let contents = "x = '☃'\ny = 2";
        let index = LineIndex::from_source_text(contents);
        assert_eq!(
            index.line_starts(),
            &[TextSize::from(0), TextSize::from(10)]
        );

        // First row.
        let loc = index.line_column(TextSize::from(0), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(0)
            }
        );

        let loc = index.line_column(TextSize::from(5), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(5)
            }
        );

        let loc = index.line_column(TextSize::from(8), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(0),
                column: OneIndexed::from_zero_indexed(6)
            }
        );

        // Second row.
        let loc = index.line_column(TextSize::from(10), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(0)
            }
        );

        // One-past-the-end.
        let loc = index.line_column(TextSize::from(15), contents);
        assert_eq!(
            loc,
            LineColumn {
                line: OneIndexed::from_zero_indexed(1),
                column: OneIndexed::from_zero_indexed(5)
            }
        );
    }
}
