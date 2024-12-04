use crate::find_newline;
use memchr::{memchr2, memrchr2};
use ruff_text_size::{TextLen, TextRange, TextSize};
use std::ops::Add;

/// Extension trait for [`str`] that provides methods for working with ranges of lines.
pub trait LineRanges {
    /// Computes the start position of the line of `offset`.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::TextSize;
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\rthird line";
    ///
    /// assert_eq!(text.line_start(TextSize::from(0)), TextSize::from(0));
    /// assert_eq!(text.line_start(TextSize::from(4)), TextSize::from(0));
    ///
    /// assert_eq!(text.line_start(TextSize::from(14)), TextSize::from(11));
    /// assert_eq!(text.line_start(TextSize::from(28)), TextSize::from(23));
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn line_start(&self, offset: TextSize) -> TextSize;

    /// Computes the start position of the file contents: either the first byte, or the byte after
    /// the BOM.
    fn bom_start_offset(&self) -> TextSize;

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
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.full_line_end(TextSize::from(3)), TextSize::from(11));
    /// assert_eq!(text.full_line_end(TextSize::from(14)), TextSize::from(24));
    /// assert_eq!(text.full_line_end(TextSize::from(28)), TextSize::from(34));
    /// ```
    ///
    /// ## Panics
    ///
    /// If `offset` is passed the end of the content.
    fn full_line_end(&self, offset: TextSize) -> TextSize;

    /// Computes the offset that is right before the newline character that ends `offset`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.line_end(TextSize::from(3)), TextSize::from(10));
    /// assert_eq!(text.line_end(TextSize::from(14)), TextSize::from(22));
    /// assert_eq!(text.line_end(TextSize::from(28)), TextSize::from(34));
    /// ```
    ///
    /// ## Panics
    ///
    /// If `offset` is passed the end of the content.
    fn line_end(&self, offset: TextSize) -> TextSize;

    /// Computes the range of this `offset`s line.
    ///
    /// The range starts at the beginning of the line and goes up to, and including, the new line character
    /// at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.full_line_range(TextSize::from(3)), TextRange::new(TextSize::from(0), TextSize::from(11)));
    /// assert_eq!(text.full_line_range(TextSize::from(14)), TextRange::new(TextSize::from(11), TextSize::from(24)));
    /// assert_eq!(text.full_line_range(TextSize::from(28)), TextRange::new(TextSize::from(24), TextSize::from(34)));
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
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.line_range(TextSize::from(3)), TextRange::new(TextSize::from(0), TextSize::from(10)));
    /// assert_eq!(text.line_range(TextSize::from(14)), TextRange::new(TextSize::from(11), TextSize::from(22)));
    /// assert_eq!(text.line_range(TextSize::from(28)), TextRange::new(TextSize::from(24), TextSize::from(34)));
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
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.full_line_str(TextSize::from(3)), "First line\n");
    /// assert_eq!(text.full_line_str(TextSize::from(14)), "second line\r\n");
    /// assert_eq!(text.full_line_str(TextSize::from(28)), "third line");
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn full_line_str(&self, offset: TextSize) -> &str;

    /// Returns the text of the `offset`'s line.
    ///
    /// Excludes the newline characters at the end of the line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.line_str(TextSize::from(3)), "First line");
    /// assert_eq!(text.line_str(TextSize::from(14)), "second line");
    /// assert_eq!(text.line_str(TextSize::from(28)), "third line");
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn line_str(&self, offset: TextSize) -> &str;

    /// Computes the range of all lines that this `range` covers.
    ///
    /// The range starts at the beginning of the line at `range.start()` and goes up to, and including, the new line character
    /// at the end of `range.ends()`'s line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     text.full_lines_range(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(11))
    /// );
    /// assert_eq!(
    ///     text.full_lines_range(TextRange::new(TextSize::from(3), TextSize::from(14))),
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
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     text.lines_range(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     TextRange::new(TextSize::from(0), TextSize::from(10))
    /// );
    /// assert_eq!(
    ///     text.lines_range(TextRange::new(TextSize::from(3), TextSize::from(14))),
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
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert!(
    ///     !text.contains_line_break(TextRange::new(TextSize::from(3), TextSize::from(5))),
    /// );
    /// assert!(
    ///     text.contains_line_break(TextRange::new(TextSize::from(3), TextSize::from(14))),
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the `range` is out of bounds.
    fn contains_line_break(&self, range: TextRange) -> bool;

    /// Returns the text of all lines that include `range`.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     text.lines_str(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     "First line"
    /// );
    /// assert_eq!(
    ///     text.lines_str(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     "First line\nsecond line"
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    fn lines_str(&self, range: TextRange) -> &str;

    /// Returns the text of all lines that include `range`.
    ///
    /// Includes the newline characters of the last line.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(
    ///     text.full_lines_str(TextRange::new(TextSize::from(3), TextSize::from(5))),
    ///     "First line\n"
    /// );
    /// assert_eq!(
    ///     text.full_lines_str(TextRange::new(TextSize::from(3), TextSize::from(14))),
    ///     "First line\nsecond line\r\n"
    /// );
    /// ```
    ///
    /// ## Panics
    /// If the start or end of `range` is out of bounds.
    fn full_lines_str(&self, range: TextRange) -> &str;

    /// Returns the zero-based index of the line containing `range`'s start.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use ruff_text_size::{Ranged, TextRange, TextSize};
    /// # use ruff_source_file::LineRanges;
    ///
    /// let text = "First line\nsecond line\r\nthird line";
    ///
    /// assert_eq!(text.count_lines_until(TextSize::from(5)), 0);
    /// assert_eq!(text.count_lines_until(TextSize::from(23)), 1);
    /// assert_eq!(text.count_lines_until(TextSize::from(24)), 2);
    /// assert_eq!(text.count_lines_until(TextSize::from(34)), 3);
    ///
    /// let text = "foo\n";
    ///
    /// assert_eq!(text.count_lines_until(TextSize::from(4)), 1);
    /// ```
    ///
    /// ## Panics
    /// If `offset` is out of bounds.
    fn count_lines_until(&self, offset: TextSize) -> u32 {
        let mut count = 0;
        let mut last_line_end = TextSize::default();

        loop {
            let line_end = self.full_line_end(last_line_end);

            if line_end <= offset && line_end != last_line_end {
                count += 1;
                last_line_end = line_end;
            } else {
                break;
            }
        }

        count
    }
}

impl LineRanges for str {
    fn line_start(&self, offset: TextSize) -> TextSize {
        let bytes = self[TextRange::up_to(offset)].as_bytes();
        if let Some(index) = memrchr2(b'\n', b'\r', bytes) {
            // SAFETY: Safe because `index < offset`
            TextSize::try_from(index).unwrap().add(TextSize::from(1))
        } else {
            self.bom_start_offset()
        }
    }

    fn bom_start_offset(&self) -> TextSize {
        if self.starts_with('\u{feff}') {
            // Skip the BOM.
            '\u{feff}'.text_len()
        } else {
            // Start of file.
            TextSize::default()
        }
    }

    fn full_line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self[usize::from(offset)..];
        if let Some((index, line_ending)) = find_newline(slice) {
            offset + TextSize::try_from(index).unwrap() + line_ending.text_len()
        } else {
            self.text_len()
        }
    }

    fn line_end(&self, offset: TextSize) -> TextSize {
        let slice = &self[offset.to_usize()..];
        if let Some(index) = memchr2(b'\n', b'\r', slice.as_bytes()) {
            offset + TextSize::try_from(index).unwrap()
        } else {
            self.text_len()
        }
    }

    fn full_line_str(&self, offset: TextSize) -> &str {
        &self[self.full_line_range(offset)]
    }

    fn line_str(&self, offset: TextSize) -> &str {
        &self[self.line_range(offset)]
    }

    fn contains_line_break(&self, range: TextRange) -> bool {
        memchr2(b'\n', b'\r', self[range].as_bytes()).is_some()
    }

    fn lines_str(&self, range: TextRange) -> &str {
        &self[self.lines_range(range)]
    }

    fn full_lines_str(&self, range: TextRange) -> &str {
        &self[self.full_lines_range(range)]
    }
}
