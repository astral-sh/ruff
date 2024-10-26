//! Struct used to efficiently slice source code at (row, column) Locations.

use memchr::{memchr2, memrchr2};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use std::ops::Add;

use crate::newlines::find_newline;

pub trait Located {
    fn as_str(&self) -> &str;

    /// Computes the start position of the line of `offset`.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::TextSize;
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\rthird line";
    ///
    /// assert_eq!(located.line_start(TextSize::from(0)), TextSize::from(0));
    /// assert_eq!(located.line_start(TextSize::from(4)), TextSize::from(0));
    ///
    /// assert_eq!(located.line_start(TextSize::from(14)), TextSize::from(11));
    /// assert_eq!(located.line_start(TextSize::from(28)), TextSize::from(23));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn line_start(&self, offset: TextSize) -> TextSize {
        let bytes = self.as_str()[TextRange::up_to(offset)].as_bytes();
        if let Some(index) = memrchr2(b'\n', b'\r', bytes) {
            // SAFETY: Safe because `index < offset`
            TextSize::try_from(index).unwrap().add(TextSize::from(1))
        } else {
            self.contents_start()
        }
    }

    /// Computes the start position of the file contents: either the first byte, or the byte after
    /// the BOM.
    fn contents_start(&self) -> TextSize {
        if self.as_str().starts_with('\u{feff}') {
            // Skip the BOM.
            '\u{feff}'.text_len()
        } else {
            // Start of file.
            TextSize::default()
        }
    }

    /// Returns `true` if `offset` is at the start of a line.
    fn is_at_start_of_line(&self, offset: TextSize) -> bool {
        self.line_start(offset) == offset
    }

    /// Computes the offset that is right after the newline character that ends `offset`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(located.full_line_end(TextSize::from(3)), TextSize::from(11));
    /// assert_eq!(located.full_line_end(TextSize::from(14)), TextSize::from(24));
    /// assert_eq!(located.full_line_end(TextSize::from(28)), TextSize::from(34));
    /// ```
    ///
    /// ## Panics
    ///
    /// If `offset` is passed the end of the content.
    fn full_line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self.as_str()[usize::from(offset)..];
        if let Some((index, line_ending)) = find_newline(slice) {
            offset + TextSize::try_from(index).unwrap() + line_ending.text_len()
        } else {
            self.as_str().text_len()
        }
    }

    /// Computes the offset that is right before the newline character that ends `offset`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(located.line_end(TextSize::from(3)), TextSize::from(10));
    /// assert_eq!(located.line_end(TextSize::from(14)), TextSize::from(22));
    /// assert_eq!(located.line_end(TextSize::from(28)), TextSize::from(34));
    /// ```
    ///
    /// ## Panics
    ///
    /// If `offset` is passed the end of the content.
    fn line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self.as_str()[usize::from(offset)..];
        if let Some(index) = memchr2(b'\n', b'\r', slice.as_bytes()) {
            offset + TextSize::try_from(index).unwrap()
        } else {
            self.as_str().text_len()
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
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(located.full_line_range(TextSize::from(3)), TextRange::new(TextSize::from(0), TextSize::from(11)));
    /// assert_eq!(located.full_line_range(TextSize::from(14)), TextRange::new(TextSize::from(11), TextSize::from(24)));
    /// assert_eq!(located.full_line_range(TextSize::from(28)), TextRange::new(TextSize::from(24), TextSize::from(34)));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn full_line_range(&self, offset: TextSize) -> TextRange {
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
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(located.line_range(TextSize::from(3)), TextRange::new(TextSize::from(0), TextSize::from(10)));
    /// assert_eq!(located.line_range(TextSize::from(14)), TextRange::new(TextSize::from(11), TextSize::from(22)));
    /// assert_eq!(located.line_range(TextSize::from(28)), TextRange::new(TextSize::from(24), TextSize::from(34)));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn line_range(&self, offset: TextSize) -> TextRange {
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
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(located.full_line_str(TextSize::from(3)), "First line\n");
    /// assert_eq!(located.full_line_str(TextSize::from(14)), "second line\r\n");
    /// assert_eq!(located.full_line_str(TextSize::from(28)), "third line");
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn full_line_str(&self, offset: TextSize) -> &str {
        &self.as_str()[self.full_line_range(offset)]
    }

    /// Returns the text of the `offset`'s line.
    ///
    /// Excludes the newline characters at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(located.line_str(TextSize::from(3)), "First line");
    /// assert_eq!(located.line_str(TextSize::from(14)), "second line");
    /// assert_eq!(located.line_str(TextSize::from(28)), "third line");
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn line_str(&self, offset: TextSize) -> &str {
        &self.as_str()[self.line_range(offset)]
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
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     located.full_lines_range(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(11))
    /// );
    /// assert_eq!(
    ///     located.full_lines_range(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(24))
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    fn full_lines_range(&self, range: TextRange) -> TextRange {
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
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     located.lines_range(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(10))
    /// );
    /// assert_eq!(
    ///     located.lines_range(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(22))
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    fn lines_range(&self, range: TextRange) -> TextRange {
        TextRange::new(self.line_start(range.start()), self.line_end(range.end()))
    }

    /// Returns true if the text of `range` contains any line break.
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert!(
    ///     !located.contains_line_break(TextRange::new(TextSize::from(3), TextSize::from(5))),
    /// );
    /// assert!(
    ///     located.contains_line_break(TextRange::new(TextSize::from(3), TextSize::from(14))),
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the `range` is out of bounds.
    fn contains_line_break(&self, range: TextRange) -> bool {
        let text = &self.as_str()[range];
        text.contains(['\n', '\r'])
    }

    /// Returns the text of all lines that include `range`.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     located.lines_str(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     "First line"
    /// );
    /// assert_eq!(
    ///     located.lines_str(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     "First line\nsecond line"
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    fn lines_str(&self, range: TextRange) -> &str {
        &self.as_str()[self.lines_range(range)]
    }

    /// Returns the text of all lines that include `range`.
    ///
    /// Includes the newline characters of the last line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::Located;
    ///
    /// let located = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     located.full_lines_str(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     "First line\n"
    /// );
    /// assert_eq!(
    ///     located.full_lines_str(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     "First line\nsecond line\r\n"
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    fn full_lines_str(&self, range: TextRange) -> &str {
        &self.as_str()[self.full_lines_range(range)]
    }

    /// Take the source code up to the given [`TextSize`].
    #[inline]
    fn up_to(&self, offset: TextSize) -> &str {
        &self.as_str()[TextRange::up_to(offset)]
    }

    /// Take the source code after the given [`TextSize`].
    #[inline]
    fn after(&self, offset: TextSize) -> &str {
        &self.as_str()[usize::from(offset)..]
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
    /// # use ruff_source_file::Located;
    ///
    /// let located = "Hello";
    ///
    /// assert_eq!(
    ///     Located::floor_char_boundary(located, TextSize::from(0)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     Located::floor_char_boundary(located, TextSize::from(5)),
    ///     TextSize::from(5)
    /// );
    ///
    /// let located = "α";
    ///
    /// assert_eq!(
    ///     Located::floor_char_boundary(located, TextSize::from(0)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     Located::floor_char_boundary(located, TextSize::from(1)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     Located::floor_char_boundary(located, TextSize::from(2)),
    ///     TextSize::from(2)
    /// );
    /// ```
    fn floor_char_boundary(&self, offset: TextSize) -> TextSize {
        if offset >= self.as_str().text_len() {
            self.as_str().text_len()
        } else {
            // We know that the character boundary is within four bytes.
            (0u32..=3u32)
                .map(TextSize::from)
                .filter_map(|index| offset.checked_sub(index))
                .find(|offset| self.as_str().is_char_boundary(offset.to_usize()))
                .unwrap_or_default()
        }
    }

    /// Take the source code between the given [`TextRange`].
    #[inline]
    fn slice<T: Ranged>(&self, ranged: T) -> &str {
        &self.as_str()[ranged.range()]
    }
}

impl Located for str {
    fn as_str(&self) -> &str {
        self
    }
}
