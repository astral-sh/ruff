use std::error::Error;
use std::fmt;
use std::hash::Hasher;
use std::num::{NonZeroU8, NonZeroU16, ParseIntError};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;
use ruff_python_trivia::{find_trailing_pragma_offset, is_pragma_comment, tab_offset};

use crate::preview::{
    is_pragma_excluded_from_import_width_enabled, is_trailing_pragma_in_line_length_enabled,
};
use crate::settings::types::PreviewMode;

/// Returns the offset within `comment` at which the pragma comment excluded from line-length
/// measurement begins, or `None` if the comment contains no such pragma.
///
/// This is the shared policy for how pragma comments (e.g., `# noqa: F401` or `# type: ignore`)
/// are excluded when measuring line width, used by `line-too-long` (E501) and
/// `doc-line-too-long` (W505), and, in preview mode, by isort's (I001) decision of whether an
/// import fits on one line (see [`LineWidthBuilder::add_comment`]). The formatter applies the
/// equivalent policy when measuring comment widths.
///
/// In stable mode, only comments that are pragmas in their entirety are excluded (the returned
/// offset is `0`). In preview mode, a trailing pragma within a mixed comment (e.g.,
/// `# explanation  # noqa: F401`) is also excluded, in which case the offset points at the `#`
/// that begins the pragma.
pub(crate) fn pragma_offset_for_line_length(comment: &str, preview: PreviewMode) -> Option<usize> {
    if is_trailing_pragma_in_line_length_enabled(preview) {
        find_trailing_pragma_offset(comment)
    } else if is_pragma_comment(comment) {
        Some(0)
    } else {
        None
    }
}

/// The length of a line of text that is considered too long.
///
/// The allowed range of values is 1..=65535
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LineLength(NonZeroU16);

impl LineLength {
    /// Maximum allowed value for a valid [`LineLength`]
    const MAX: u16 = u16::MAX;

    /// Return the numeric value for this [`LineLength`]
    pub fn value(&self) -> u16 {
        self.0.get()
    }
}

impl Default for LineLength {
    fn default() -> Self {
        Self(NonZeroU16::new(88).unwrap())
    }
}

impl fmt::Display for LineLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<'de> serde::Deserialize<'de> for LineLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;

        u16::try_from(value)
            .ok()
            .and_then(|u16_value| Self::try_from(u16_value).ok())
            .ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "line-length must be between 1 and {} (got {value})",
                    Self::MAX,
                ))
            })
    }
}

impl CacheKey for LineLength {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u16(self.0.get());
    }
}

/// Error type returned when parsing a [`LineLength`] from a string fails
pub enum ParseLineWidthError {
    /// The string could not be parsed as a valid [u16]
    ParseError(ParseIntError),
    /// The [u16] value of the string is not a valid [`LineLength`]
    TryFromIntError(LineLengthFromIntError),
}

impl std::fmt::Debug for ParseLineWidthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for ParseLineWidthError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseLineWidthError::ParseError(err) => std::fmt::Display::fmt(err, fmt),
            ParseLineWidthError::TryFromIntError(err) => std::fmt::Display::fmt(err, fmt),
        }
    }
}

impl Error for ParseLineWidthError {}

impl FromStr for LineLength {
    type Err = ParseLineWidthError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = u16::from_str(s).map_err(ParseLineWidthError::ParseError)?;
        let value = Self::try_from(value).map_err(ParseLineWidthError::TryFromIntError)?;
        Ok(value)
    }
}

/// Error type returned when converting a u16 to a [`LineLength`] fails
#[derive(Clone, Copy, Debug)]
pub struct LineLengthFromIntError(pub u16);

impl TryFrom<u16> for LineLength {
    type Error = LineLengthFromIntError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match NonZeroU16::try_from(value) {
            Ok(value) => Ok(LineLength(value)),
            Err(_) => Err(LineLengthFromIntError(value)),
        }
    }
}

impl std::fmt::Display for LineLengthFromIntError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "The line width must be a value between 1 and {}.",
            LineLength::MAX
        )
    }
}

impl From<LineLength> for u16 {
    fn from(value: LineLength) -> Self {
        value.0.get()
    }
}

impl From<LineLength> for NonZeroU16 {
    fn from(value: LineLength) -> Self {
        value.0
    }
}

/// A measure of the width of a line of text.
///
/// This is used to determine if a line is too long.
/// It should be compared to a [`LineLength`].
#[derive(Clone, Copy, Debug)]
pub struct LineWidthBuilder {
    /// The width of the line.
    width: usize,
    /// The column of the line.
    /// This is used to calculate the width of tabs.
    column: usize,
    /// The tab size to use when calculating the width of tabs.
    tab_size: IndentWidth,
}

impl Default for LineWidthBuilder {
    fn default() -> Self {
        Self::new(IndentWidth::default())
    }
}

impl PartialEq for LineWidthBuilder {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
    }
}

impl Eq for LineWidthBuilder {}

impl PartialOrd for LineWidthBuilder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LineWidthBuilder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.width.cmp(&other.width)
    }
}

impl LineWidthBuilder {
    pub(crate) fn get(&self) -> usize {
        self.width
    }

    /// Creates a new `LineWidth` with the given tab size.
    pub(crate) fn new(tab_size: IndentWidth) -> Self {
        LineWidthBuilder {
            width: 0,
            column: 0,
            tab_size,
        }
    }

    fn update(mut self, chars: impl Iterator<Item = char>) -> Self {
        let tab_size: usize = self.tab_size.as_usize();
        for c in chars {
            match c {
                '\t' => {
                    let tab_offset = tab_offset(self.column, tab_size);
                    self.width += tab_offset;
                    self.column += tab_offset;
                }
                '\n' | '\r' => {
                    self.width = 0;
                    self.column = 0;
                }
                _ => {
                    self.width += c.width().unwrap_or(0);
                    self.column += 1;
                }
            }
        }
        self
    }

    /// Adds the given text to the line width.
    #[must_use]
    pub(crate) fn add_str(self, text: &str) -> Self {
        self.update(text.chars())
    }

    /// Adds the given character to the line width.
    #[must_use]
    pub(crate) fn add_char(self, c: char) -> Self {
        self.update(std::iter::once(c))
    }

    /// Adds the given width to the line width.
    /// Also adds the given width to the column.
    /// It is generally better to use [`LineWidthBuilder::add_str`] or [`LineWidthBuilder::add_char`].
    /// The width and column should be the same for the corresponding text.
    /// Currently, this is only used to add spaces.
    #[must_use]
    pub(crate) fn add_width(mut self, width: usize) -> Self {
        self.width += width;
        self.column += width;
        self
    }

    /// Adds the width of a trailing comment, including the standard two-space separator that
    /// precedes it. In preview mode, any pragma comment is excluded per
    /// [`pragma_offset_for_line_length`].
    ///
    /// Pragma comments are excluded so that adding one to a line never affects whether the line
    /// is considered to fit, consistent with how `line-too-long` (E501) measures lines. For
    /// example, counting a `# noqa` comment towards an import's width could cause isort to wrap
    /// an import that otherwise fits on one line, moving the pragma to a position where it no
    /// longer applies to the import statement:
    ///
    /// ```python
    /// from module import (
    ///     member,  # noqa: PLC0415
    /// )
    /// ```
    ///
    /// Unlike E501, which has always stripped whole-pragma comments on stable, the exclusion
    /// changes how imports are formatted, so it is preview-gated in its entirety: on stable, the
    /// full comment width is counted.
    #[must_use]
    pub(crate) fn add_comment(self, comment: &str, preview: PreviewMode) -> Self {
        if !is_pragma_excluded_from_import_width_enabled(preview) {
            return self.add_width(2).add_str(comment);
        }
        let counted = match pragma_offset_for_line_length(comment, preview) {
            Some(offset) => comment[..offset].trim_end(),
            None => comment,
        };
        if counted.is_empty() {
            self
        } else {
            self.add_width(2).add_str(counted)
        }
    }
}

impl PartialEq<LineLength> for LineWidthBuilder {
    fn eq(&self, other: &LineLength) -> bool {
        self.width == (other.value() as usize)
    }
}

impl PartialOrd<LineLength> for LineWidthBuilder {
    fn partial_cmp(&self, other: &LineLength) -> Option<std::cmp::Ordering> {
        self.width.partial_cmp(&(other.value() as usize))
    }
}

/// The size of a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IndentWidth(NonZeroU8);

impl IndentWidth {
    pub(crate) fn as_usize(self) -> usize {
        self.0.get() as usize
    }
}

impl Default for IndentWidth {
    fn default() -> Self {
        Self(NonZeroU8::new(4).unwrap())
    }
}

impl fmt::Display for IndentWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<NonZeroU8> for IndentWidth {
    fn from(tab_size: NonZeroU8) -> Self {
        Self(tab_size)
    }
}

impl From<IndentWidth> for NonZeroU8 {
    fn from(value: IndentWidth) -> Self {
        value.0
    }
}
