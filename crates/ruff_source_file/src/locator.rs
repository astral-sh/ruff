//! Struct used to efficiently slice source code at (row, column) Locations.

use std::ops::Add;

use memchr::{memchr2, memrchr2};
use once_cell::unsync::OnceCell;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::newlines::find_newline;
use crate::{LineIndex, OneIndexed, SourceCode, SourceLocation};

pub struct Locator<'a> {
    contents: &'a str,
    index: OnceCell<LineIndex>,
}

impl<'a> Locator<'a> {
    pub const fn new(contents: &'a str) -> Self {
        Self {
            contents,
            index: OnceCell::new(),
        }
    }

    #[deprecated(
        note = "This is expensive, avoid using outside of the diagnostic phase. Prefer the other `Locator` methods instead."
    )]
    pub fn compute_line_index(&self, offset: TextSize) -> OneIndexed {
        self.to_index().line_index(offset)
    }

    #[deprecated(
        note = "This is expensive, avoid using outside of the diagnostic phase. Prefer the other `Locator` methods instead."
    )]
    pub fn compute_source_location(&self, offset: TextSize) -> SourceLocation {
        self.to_source_code().source_location(offset)
    }

    fn to_index(&self) -> &LineIndex {
        self.index
            .get_or_init(|| LineIndex::from_source_text(self.contents))
    }

    pub fn line_index(&self) -> Option<&LineIndex> {
        self.index.get()
    }

    pub fn to_source_code(&self) -> SourceCode {
        SourceCode {
            index: self.to_index(),
            text: self.contents,
        }
    }

    /// Computes the start position of the line of `offset`.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::TextSize;
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\rthird line");
    ///
    /// assert_eq!(locator.line_start(TextSize::from(0)), TextSize::from(0));
    /// assert_eq!(locator.line_start(TextSize::from(4)), TextSize::from(0));
    ///
    /// assert_eq!(locator.line_start(TextSize::from(14)), TextSize::from(11));
    /// assert_eq!(locator.line_start(TextSize::from(28)), TextSize::from(23));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    pub fn line_start(&self, offset: TextSize) -> TextSize {
        let bytes = self.contents[TextRange::up_to(offset)].as_bytes();
        if let Some(index) = memrchr2(b'\n', b'\r', bytes) {
            // SAFETY: Safe because `index < offset`
            TextSize::try_from(index).unwrap().add(TextSize::from(1))
        } else {
            self.contents_start()
        }
    }

    /// Computes the start position of the file contents: either the first byte, or the byte after
    /// the BOM.
    pub fn contents_start(&self) -> TextSize {
        if self.contents.starts_with('\u{feff}') {
            // Skip the BOM.
            '\u{feff}'.text_len()
        } else {
            // Start of file.
            TextSize::default()
        }
    }

    /// Returns `true` if `offset` is at the start of a line.
    pub fn is_at_start_of_line(&self, offset: TextSize) -> bool {
        self.line_start(offset) == offset
    }

    /// Computes the offset that is right after the newline character that ends `offset`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(locator.full_line_end(TextSize::from(3)), TextSize::from(11));
    /// assert_eq!(locator.full_line_end(TextSize::from(14)), TextSize::from(24));
    /// assert_eq!(locator.full_line_end(TextSize::from(28)), TextSize::from(34));
    /// ```
    ///
    /// ## Panics
    ///
    /// If `offset` is passed the end of the content.
    pub fn full_line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self.contents[usize::from(offset)..];
        if let Some((index, line_ending)) = find_newline(slice) {
            offset + TextSize::try_from(index).unwrap() + line_ending.text_len()
        } else {
            self.contents.text_len()
        }
    }

    /// Computes the offset that is right before the newline character that ends `offset`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(locator.line_end(TextSize::from(3)), TextSize::from(10));
    /// assert_eq!(locator.line_end(TextSize::from(14)), TextSize::from(22));
    /// assert_eq!(locator.line_end(TextSize::from(28)), TextSize::from(34));
    /// ```
    ///
    /// ## Panics
    ///
    /// If `offset` is passed the end of the content.
    pub fn line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self.contents[usize::from(offset)..];
        if let Some(index) = memchr2(b'\n', b'\r', slice.as_bytes()) {
            offset + TextSize::try_from(index).unwrap()
        } else {
            self.contents.text_len()
        }
    }

    /// Computes the range of this `offset`s line.
    ///
    /// The range starts at the beginning of the line and goes up to, and including, the new line character
    /// at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(locator.full_line_range(TextSize::from(3)), TextRange::new(TextSize::from(0), TextSize::from(11)));
    /// assert_eq!(locator.full_line_range(TextSize::from(14)), TextRange::new(TextSize::from(11), TextSize::from(24)));
    /// assert_eq!(locator.full_line_range(TextSize::from(28)), TextRange::new(TextSize::from(24), TextSize::from(34)));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    pub fn full_line_range(&self, offset: TextSize) -> TextRange {
        TextRange::new(self.line_start(offset), self.full_line_end(offset))
    }

    /// Computes the range of this `offset`s line ending before the newline character.
    ///
    /// The range starts at the beginning of the line and goes up to, but excluding, the new line character
    /// at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(locator.line_range(TextSize::from(3)), TextRange::new(TextSize::from(0), TextSize::from(10)));
    /// assert_eq!(locator.line_range(TextSize::from(14)), TextRange::new(TextSize::from(11), TextSize::from(22)));
    /// assert_eq!(locator.line_range(TextSize::from(28)), TextRange::new(TextSize::from(24), TextSize::from(34)));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    pub fn line_range(&self, offset: TextSize) -> TextRange {
        TextRange::new(self.line_start(offset), self.line_end(offset))
    }

    /// Returns the text of the `offset`'s line.
    ///
    /// The line includes the newline characters at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(locator.full_line(TextSize::from(3)), "First line\n");
    /// assert_eq!(locator.full_line(TextSize::from(14)), "second line\r\n");
    /// assert_eq!(locator.full_line(TextSize::from(28)), "third line");
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    pub fn full_line(&self, offset: TextSize) -> &'a str {
        &self.contents[self.full_line_range(offset)]
    }

    /// Returns the text of the `offset`'s line.
    ///
    /// Excludes the newline characters at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(locator.line(TextSize::from(3)), "First line");
    /// assert_eq!(locator.line(TextSize::from(14)), "second line");
    /// assert_eq!(locator.line(TextSize::from(28)), "third line");
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    pub fn line(&self, offset: TextSize) -> &'a str {
        &self.contents[self.line_range(offset)]
    }

    /// Computes the range of all lines that this `range` covers.
    ///
    /// The range starts at the beginning of the line at `range.start()` and goes up to, and including, the new line character
    /// at the end of `range.ends()`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(
    ///     locator.full_lines_range(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(11))
    /// );
    /// assert_eq!(
    ///     locator.full_lines_range(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(24))
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    pub fn full_lines_range(&self, range: TextRange) -> TextRange {
        TextRange::new(
            self.line_start(range.start()),
            self.full_line_end(range.end()),
        )
    }

    /// Computes the range of all lines that this `range` covers.
    ///
    /// The range starts at the beginning of the line at `range.start()` and goes up to, but excluding, the new line character
    /// at the end of `range.end()`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(
    ///     locator.lines_range(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(10))
    /// );
    /// assert_eq!(
    ///     locator.lines_range(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(22))
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    pub fn lines_range(&self, range: TextRange) -> TextRange {
        TextRange::new(self.line_start(range.start()), self.line_end(range.end()))
    }

    /// Returns true if the text of `range` contains any line break.
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert!(
    ///     !locator.contains_line_break(TextRange::new(TextSize::from(3), TextSize::from(5))),
    /// );
    /// assert!(
    ///     locator.contains_line_break(TextRange::new(TextSize::from(3), TextSize::from(14))),
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the `range` is out of bounds.
    pub fn contains_line_break(&self, range: TextRange) -> bool {
        let text = &self.contents[range];
        text.contains(['\n', '\r'])
    }

    /// Returns the text of all lines that include `range`.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(
    ///     locator.lines(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     "First line"
    /// );
    /// assert_eq!(
    ///     locator.lines(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     "First line\nsecond line"
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    pub fn lines(&self, range: TextRange) -> &'a str {
        &self.contents[self.lines_range(range)]
    }

    /// Returns the text of all lines that include `range`.
    ///
    /// Includes the newline characters of the last line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("First line\nsecond line\r\nthird line");
    ///
    /// assert_eq!(
    ///     locator.full_lines(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     "First line\n"
    /// );
    /// assert_eq!(
    ///     locator.full_lines(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     "First line\nsecond line\r\n"
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    pub fn full_lines(&self, range: TextRange) -> &'a str {
        &self.contents[self.full_lines_range(range)]
    }

    /// Take the source code up to the given [`TextSize`].
    #[inline]
    pub fn up_to(&self, offset: TextSize) -> &'a str {
        &self.contents[TextRange::up_to(offset)]
    }

    /// Take the source code after the given [`TextSize`].
    #[inline]
    pub fn after(&self, offset: TextSize) -> &'a str {
        &self.contents[usize::from(offset)..]
    }

    /// Finds the closest [`TextSize`] not exceeding the offset for which `is_char_boundary` is
    /// `true`.
    ///
    /// Can be replaced with `str::floor_char_boundary` once it's stable.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let locator = Locator::new("Hello");
    ///
    /// assert_eq!(
    ///     locator.floor_char_boundary(TextSize::from(0)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     locator.floor_char_boundary(TextSize::from(5)),
    ///     TextSize::from(5)
    /// );
    ///
    /// let locator = Locator::new("α");
    ///
    /// assert_eq!(
    ///     locator.floor_char_boundary(TextSize::from(0)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     locator.floor_char_boundary(TextSize::from(1)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     locator.floor_char_boundary(TextSize::from(2)),
    ///     TextSize::from(2)
    /// );
    /// ```
    pub fn floor_char_boundary(&self, offset: TextSize) -> TextSize {
        if offset >= self.text_len() {
            self.text_len()
        } else {
            // We know that the character boundary is within four bytes.
            (0u32..=3u32)
                .map(TextSize::from)
                .filter_map(|index| offset.checked_sub(index))
                .find(|offset| self.contents.is_char_boundary(offset.to_usize()))
                .unwrap_or_default()
        }
    }

    /// Compute the byte offset from language server protocol zero-indexed row and utf-16 column
    /// indices.
    ///
    /// It's possible to negotiate the text encoding with the LSP client, but the default that must
    /// always be supported and that we currently use is utf-16.
    ///
    /// We get row and column from the LSP. E.g.
    /// ```text
    /// a=(1,2,)
    /// b=(3,4,)
    ///   ^
    /// c=(5,6,)
    /// ```
    /// has coordinates `1:2`. Note that indices are computed in utf-16, e.g.
    /// ```text
    /// "안녕"
    ///    ^
    /// ```
    /// where the first syllable is a single character (two bytes), we get `0:2`, while for
    /// ```text
    /// "감기"
    ///    ^
    /// ```
    /// where the first syllable is three characters (three times two bytes), we get `0:4`. But for
    /// ```text
    /// 豆腐
    ///   ^
    /// ```
    /// we get `0:2` because `豆` is two characters (4 bytes) in utf-16.
    ///
    /// ```rust
    /// # use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
    /// # use ruff_source_file::Locator;
    ///
    /// let source = "a=(1,2,)\nb=(3,4,)";
    /// let locator = Locator::new(source);
    /// let offset = locator.convert_row_and_column_utf16(1, 2).unwrap();
    /// assert_eq!(&source[TextRange::new(offset, source.text_len())], "(3,4,)");
    ///
    /// let source = "a=(1,2,)\n'안녕'";
    /// let locator = Locator::new(source);
    /// let offset = locator.convert_row_and_column_utf16(1, 2).unwrap();
    /// assert_eq!(&source[TextRange::new(offset, source.text_len())], "녕'");
    ///
    /// let source = "a=(1,2,)\n'감기'";
    /// let locator = Locator::new(source);
    /// let offset = locator.convert_row_and_column_utf16(1, 4).unwrap();
    /// assert_eq!(&source[TextRange::new(offset, source.text_len())], "기'");
    ///
    /// let source = "a=(1,2,)\n'豆腐'";
    /// let locator = Locator::new(source);
    /// let offset = locator.convert_row_and_column_utf16(1, 2).unwrap();
    /// assert_eq!(&source[TextRange::new(offset, source.text_len())], "腐'");
    /// ```
    pub fn convert_row_and_column_utf16(&self, row: usize, column: usize) -> Option<TextSize> {
        let line_start = *self.to_index().line_starts().get(row)?;
        let next_line_start = self
            .to_index()
            .line_starts()
            .get(row + 1)
            .copied()
            .unwrap_or(self.contents.text_len());
        let line_contents = &self.contents[TextRange::from(line_start..next_line_start)];

        let mut len_bytes = TextSize::default();
        let mut len_utf16 = 0;
        for char in line_contents
            .chars()
            // Since the range goes to the next line start, `line_contents` contains the line
            // break
            .take_while(|c| *c != '\n' && *c != '\r')
        {
            // This check must be first for the 0 column case
            if len_utf16 >= column {
                break;
            }
            len_bytes += char.text_len();
            len_utf16 += char.len_utf16();
        }
        if len_utf16 != column {
            return None;
        }

        Some(line_start + len_bytes)
    }

    /// Take the source code between the given [`TextRange`].
    #[inline]
    pub fn slice<T: Ranged>(&self, ranged: T) -> &'a str {
        &self.contents[ranged.range()]
    }

    /// Return the underlying source code.
    pub fn contents(&self) -> &'a str {
        self.contents
    }

    /// Return the number of bytes in the source code.
    pub const fn len(&self) -> usize {
        self.contents.len()
    }

    pub fn text_len(&self) -> TextSize {
        self.contents.text_len()
    }

    /// Return `true` if the source code is empty.
    pub const fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}
