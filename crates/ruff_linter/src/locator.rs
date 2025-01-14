//! Struct used to efficiently slice source code at (row, column) Locations.

use std::cell::OnceCell;

use ruff_source_file::{LineIndex, LineRanges, OneIndexed, SourceCode, SourceLocation};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

#[derive(Debug)]
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

    pub fn with_index(contents: &'a str, index: LineIndex) -> Self {
        Self {
            contents,
            index: OnceCell::from(index),
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

    pub fn to_index(&self) -> &LineIndex {
        self.index
            .get_or_init(|| LineIndex::from_source_text(self.contents))
    }

    pub fn line_index(&self) -> Option<&LineIndex> {
        self.index.get()
    }

    pub fn to_source_code(&self) -> SourceCode {
        SourceCode::new(self.contents, self.to_index())
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
    /// # use ruff_linter::Locator;
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

    /// Finds the closest [`TextSize`] not less than the offset given for which
    /// `is_char_boundary` is `true`. Unless the offset given is greater than
    /// the length of the underlying contents, in which case, the length of the
    /// contents is returned.
    ///
    /// Can be replaced with `str::ceil_char_boundary` once it's stable.
    ///
    /// # Examples
    ///
    /// From `std`:
    ///
    /// ```
    /// use ruff_text_size::{Ranged, TextSize};
    /// use ruff_linter::Locator;
    ///
    /// let locator = Locator::new("❤️🧡💛💚💙💜");
    /// assert_eq!(locator.text_len(), TextSize::from(26));
    /// assert!(!locator.contents().is_char_boundary(13));
    ///
    /// let closest = locator.ceil_char_boundary(TextSize::from(13));
    /// assert_eq!(closest, TextSize::from(14));
    /// assert_eq!(&locator.contents()[..closest.to_usize()], "❤️🧡💛");
    /// ```
    ///
    /// Additional examples:
    ///
    /// ```
    /// use ruff_text_size::{Ranged, TextRange, TextSize};
    /// use ruff_linter::Locator;
    ///
    /// let locator = Locator::new("Hello");
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(0)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(5)),
    ///     TextSize::from(5)
    /// );
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(6)),
    ///     TextSize::from(5)
    /// );
    ///
    /// let locator = Locator::new("α");
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(0)),
    ///     TextSize::from(0)
    /// );
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(1)),
    ///     TextSize::from(2)
    /// );
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(2)),
    ///     TextSize::from(2)
    /// );
    ///
    /// assert_eq!(
    ///     locator.ceil_char_boundary(TextSize::from(3)),
    ///     TextSize::from(2)
    /// );
    /// ```
    pub fn ceil_char_boundary(&self, offset: TextSize) -> TextSize {
        let upper_bound = offset
            .to_u32()
            .saturating_add(4)
            .min(self.text_len().to_u32());
        (offset.to_u32()..upper_bound)
            .map(TextSize::from)
            .find(|offset| self.contents.is_char_boundary(offset.to_usize()))
            .unwrap_or_else(|| TextSize::from(upper_bound))
    }

    /// Take the source code between the given [`TextRange`].
    #[inline]
    pub fn slice<T: Ranged>(&self, ranged: T) -> &'a str {
        &self.contents[ranged.range()]
    }

    /// Return the underlying source code.
    pub const fn contents(&self) -> &'a str {
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

// Override the `_str` methods from [`LineRanges`] to extend the lifetime to `'a`.
impl<'a> Locator<'a> {
    /// Returns the text of the `offset`'s line.
    ///
    /// See [`LineRanges::full_lines_str`].
    pub fn full_line_str(&self, offset: TextSize) -> &'a str {
        self.contents.full_line_str(offset)
    }

    /// Returns the text of the `offset`'s line.
    ///
    /// See [`LineRanges::line_str`].
    pub fn line_str(&self, offset: TextSize) -> &'a str {
        self.contents.line_str(offset)
    }

    /// Returns the text of all lines that include `range`.
    ///
    /// See [`LineRanges::lines_str`].
    pub fn lines_str(&self, range: TextRange) -> &'a str {
        self.contents.lines_str(range)
    }

    /// Returns the text of all lines that include `range`.
    ///
    /// See [`LineRanges::full_lines_str`].
    pub fn full_lines_str(&self, range: TextRange) -> &'a str {
        self.contents.full_lines_str(range)
    }
}

// Allow calling [`LineRanges`] methods on [`Locator`] directly.
impl LineRanges for Locator<'_> {
    #[inline]
    fn line_start(&self, offset: TextSize) -> TextSize {
        self.contents.line_start(offset)
    }

    #[inline]
    fn bom_start_offset(&self) -> TextSize {
        self.contents.bom_start_offset()
    }

    #[inline]
    fn full_line_end(&self, offset: TextSize) -> TextSize {
        self.contents.full_line_end(offset)
    }

    #[inline]
    fn line_end(&self, offset: TextSize) -> TextSize {
        self.contents.line_end(offset)
    }

    #[inline]
    fn full_line_str(&self, offset: TextSize) -> &str {
        self.contents.full_line_str(offset)
    }

    #[inline]
    fn line_str(&self, offset: TextSize) -> &str {
        self.contents.line_str(offset)
    }

    #[inline]
    fn contains_line_break(&self, range: TextRange) -> bool {
        self.contents.contains_line_break(range)
    }

    #[inline]
    fn lines_str(&self, range: TextRange) -> &str {
        self.contents.lines_str(range)
    }

    #[inline]
    fn full_lines_str(&self, range: TextRange) -> &str {
        self.contents.full_lines_str(range)
    }
}
