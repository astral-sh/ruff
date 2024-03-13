use std::iter::FusedIterator;
use std::ops::Deref;

use memchr::{memchr2, memrchr2};
use ruff_text_size::{TextLen, TextRange, TextSize};

/// Extension trait for [`str`] that provides a [`UniversalNewlineIterator`].
pub trait UniversalNewlines {
    fn universal_newlines(&self) -> UniversalNewlineIterator<'_>;
}

impl UniversalNewlines for str {
    fn universal_newlines(&self) -> UniversalNewlineIterator<'_> {
        UniversalNewlineIterator::from(self)
    }
}

/// Like [`str::lines`], but accommodates LF, CRLF, and CR line endings,
/// the latter of which are not supported by [`str::lines`].
///
/// ## Examples
///
/// ```rust
/// # use ruff_text_size::TextSize;
/// # use ruff_source_file::{Line, UniversalNewlineIterator};
/// let mut lines = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop");
///
/// assert_eq!(lines.next_back(), Some(Line::new("bop", TextSize::from(14))));
/// assert_eq!(lines.next(), Some(Line::new("foo\n", TextSize::from(0))));
/// assert_eq!(lines.next_back(), Some(Line::new("baz\r", TextSize::from(10))));
/// assert_eq!(lines.next(), Some(Line::new("bar\n", TextSize::from(4))));
/// assert_eq!(lines.next_back(), Some(Line::new("\r\n", TextSize::from(8))));
/// assert_eq!(lines.next(), None);
/// ```
#[derive(Clone)]
pub struct UniversalNewlineIterator<'a> {
    text: &'a str,
    offset: TextSize,
    offset_back: TextSize,
}

impl<'a> UniversalNewlineIterator<'a> {
    pub fn with_offset(text: &'a str, offset: TextSize) -> UniversalNewlineIterator<'a> {
        UniversalNewlineIterator {
            text,
            offset,
            offset_back: offset + text.text_len(),
        }
    }

    pub fn from(text: &'a str) -> UniversalNewlineIterator<'a> {
        Self::with_offset(text, TextSize::default())
    }
}

/// Finds the next newline character. Returns its position and the [`LineEnding`].
#[inline]
pub fn find_newline(text: &str) -> Option<(usize, LineEnding)> {
    let bytes = text.as_bytes();
    if let Some(position) = memchr2(b'\n', b'\r', bytes) {
        let line_ending = match bytes[position] {
            // Explicit branch for `\n` as this is the most likely path
            b'\n' => LineEnding::Lf,
            // '\r\n'
            b'\r' if bytes.get(position.saturating_add(1)) == Some(&b'\n') => LineEnding::CrLf,
            // '\r'
            _ => LineEnding::Cr,
        };

        Some((position, line_ending))
    } else {
        None
    }
}

impl<'a> Iterator for UniversalNewlineIterator<'a> {
    type Item = Line<'a>;

    #[inline]
    fn next(&mut self) -> Option<Line<'a>> {
        if self.text.is_empty() {
            return None;
        }

        let line = if let Some((newline_position, line_ending)) = find_newline(self.text) {
            let (text, remainder) = self.text.split_at(newline_position + line_ending.len());

            let line = Line {
                offset: self.offset,
                text,
            };

            self.text = remainder;
            self.offset += text.text_len();

            line
        }
        // Last line
        else {
            Line {
                offset: self.offset,
                text: std::mem::take(&mut self.text),
            }
        };

        Some(line)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for UniversalNewlineIterator<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.text.is_empty() {
            return None;
        }

        let len = self.text.len();

        // Trim any trailing newlines.
        let haystack = match self.text.as_bytes()[len - 1] {
            b'\n' if len > 1 && self.text.as_bytes()[len - 2] == b'\r' => &self.text[..len - 2],
            b'\n' | b'\r' => &self.text[..len - 1],
            _ => self.text,
        };

        // Find the end of the previous line. The previous line is the text up to, but not including
        // the newline character.
        let line = if let Some(line_end) = memrchr2(b'\n', b'\r', haystack.as_bytes()) {
            // '\n' or '\r' or '\r\n'
            let (remainder, line) = self.text.split_at(line_end + 1);
            self.text = remainder;
            self.offset_back -= line.text_len();

            Line {
                text: line,
                offset: self.offset_back,
            }
        } else {
            // Last line
            let offset = self.offset_back - self.text.text_len();
            Line {
                text: std::mem::take(&mut self.text),
                offset,
            }
        };

        Some(line)
    }
}

impl FusedIterator for UniversalNewlineIterator<'_> {}

/// Like [`UniversalNewlineIterator`], but includes a trailing newline as an empty line.
pub struct NewlineWithTrailingNewline<'a> {
    trailing: Option<Line<'a>>,
    underlying: UniversalNewlineIterator<'a>,
}

impl<'a> NewlineWithTrailingNewline<'a> {
    pub fn from(input: &'a str) -> NewlineWithTrailingNewline<'a> {
        Self::with_offset(input, TextSize::default())
    }

    pub fn with_offset(input: &'a str, offset: TextSize) -> Self {
        NewlineWithTrailingNewline {
            underlying: UniversalNewlineIterator::with_offset(input, offset),
            trailing: if input.ends_with(['\r', '\n']) {
                Some(Line {
                    text: "",
                    offset: offset + input.text_len(),
                })
            } else {
                None
            },
        }
    }
}

impl<'a> Iterator for NewlineWithTrailingNewline<'a> {
    type Item = Line<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.underlying.next().or_else(|| self.trailing.take())
    }
}

impl DoubleEndedIterator for NewlineWithTrailingNewline<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.trailing.take().or_else(|| self.underlying.next_back())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Line<'a> {
    text: &'a str,
    offset: TextSize,
}

impl<'a> Line<'a> {
    pub fn new(text: &'a str, offset: TextSize) -> Self {
        Self { text, offset }
    }

    #[inline]
    pub const fn start(&self) -> TextSize {
        self.offset
    }

    /// Returns the byte offset where the line ends, including its terminating new line character.
    #[inline]
    pub fn full_end(&self) -> TextSize {
        self.offset + self.full_text_len()
    }

    /// Returns the byte offset where the line ends, excluding its new line character
    #[inline]
    pub fn end(&self) -> TextSize {
        self.offset + self.as_str().text_len()
    }

    /// Returns the range of the line, including its terminating new line character.
    #[inline]
    pub fn full_range(&self) -> TextRange {
        TextRange::at(self.offset, self.text.text_len())
    }

    /// Returns the range of the line, excluding its terminating new line character
    #[inline]
    pub fn range(&self) -> TextRange {
        TextRange::new(self.start(), self.end())
    }

    /// Returns the line's new line character, if any.
    #[inline]
    pub fn line_ending(&self) -> Option<LineEnding> {
        let mut bytes = self.text.bytes().rev();
        match bytes.next() {
            Some(b'\n') => {
                if bytes.next() == Some(b'\r') {
                    Some(LineEnding::CrLf)
                } else {
                    Some(LineEnding::Lf)
                }
            }
            Some(b'\r') => Some(LineEnding::Cr),
            _ => None,
        }
    }

    /// Returns the text of the line, excluding the terminating new line character.
    #[inline]
    pub fn as_str(&self) -> &'a str {
        let newline_len = self
            .line_ending()
            .map_or(0, |line_ending| line_ending.len());
        &self.text[..self.text.len() - newline_len]
    }

    /// Returns the line's text, including the terminating new line character.
    #[inline]
    pub fn as_full_str(&self) -> &'a str {
        self.text
    }

    #[inline]
    pub fn full_text_len(&self) -> TextSize {
        self.text.text_len()
    }
}

impl Deref for Line<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<&str> for Line<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Line<'_>> for &str {
    fn eq(&self, other: &Line<'_>) -> bool {
        *self == other.as_str()
    }
}

/// The line ending style used in Python source code.
/// See <https://docs.python.org/3/reference/lexical_analysis.html#physical-lines>
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LineEnding {
    Lf,
    Cr,
    CrLf,
}

impl Default for LineEnding {
    fn default() -> Self {
        if cfg!(windows) {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        }
    }
}

impl LineEnding {
    pub const fn as_str(&self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        match self {
            LineEnding::Lf | LineEnding::Cr => 1,
            LineEnding::CrLf => 2,
        }
    }

    pub const fn text_len(&self) -> TextSize {
        match self {
            LineEnding::Lf | LineEnding::Cr => TextSize::new(1),
            LineEnding::CrLf => TextSize::new(2),
        }
    }
}

impl Deref for LineEnding {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextSize;

    use super::{Line, UniversalNewlineIterator};

    #[test]
    fn universal_newlines_empty_str() {
        let lines: Vec<_> = UniversalNewlineIterator::from("").collect();
        assert_eq!(lines, Vec::<Line>::new());

        let lines: Vec<_> = UniversalNewlineIterator::from("").rev().collect();
        assert_eq!(lines, Vec::<Line>::new());
    }

    #[test]
    fn universal_newlines_forward() {
        let lines: Vec<_> = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop").collect();
        assert_eq!(
            lines,
            vec![
                Line::new("foo\n", TextSize::from(0)),
                Line::new("bar\n", TextSize::from(4)),
                Line::new("\r\n", TextSize::from(8)),
                Line::new("baz\r", TextSize::from(10)),
                Line::new("bop", TextSize::from(14)),
            ]
        );

        let lines: Vec<_> = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop\n").collect();
        assert_eq!(
            lines,
            vec![
                Line::new("foo\n", TextSize::from(0)),
                Line::new("bar\n", TextSize::from(4)),
                Line::new("\r\n", TextSize::from(8)),
                Line::new("baz\r", TextSize::from(10)),
                Line::new("bop\n", TextSize::from(14)),
            ]
        );

        let lines: Vec<_> = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop\n\n").collect();
        assert_eq!(
            lines,
            vec![
                Line::new("foo\n", TextSize::from(0)),
                Line::new("bar\n", TextSize::from(4)),
                Line::new("\r\n", TextSize::from(8)),
                Line::new("baz\r", TextSize::from(10)),
                Line::new("bop\n", TextSize::from(14)),
                Line::new("\n", TextSize::from(18)),
            ]
        );
    }

    #[test]
    fn universal_newlines_backwards() {
        let lines: Vec<_> = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop")
            .rev()
            .collect();
        assert_eq!(
            lines,
            vec![
                Line::new("bop", TextSize::from(14)),
                Line::new("baz\r", TextSize::from(10)),
                Line::new("\r\n", TextSize::from(8)),
                Line::new("bar\n", TextSize::from(4)),
                Line::new("foo\n", TextSize::from(0)),
            ]
        );

        let lines: Vec<_> = UniversalNewlineIterator::from("foo\nbar\n\nbaz\rbop\n")
            .rev()
            .map(|line| line.as_str())
            .collect();

        assert_eq!(
            lines,
            vec![
                Line::new("bop\n", TextSize::from(13)),
                Line::new("baz\r", TextSize::from(9)),
                Line::new("\n", TextSize::from(8)),
                Line::new("bar\n", TextSize::from(4)),
                Line::new("foo\n", TextSize::from(0)),
            ]
        );
    }

    #[test]
    fn universal_newlines_mixed() {
        let mut lines = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop");

        assert_eq!(
            lines.next_back(),
            Some(Line::new("bop", TextSize::from(14)))
        );
        assert_eq!(lines.next(), Some(Line::new("foo\n", TextSize::from(0))));
        assert_eq!(
            lines.next_back(),
            Some(Line::new("baz\r", TextSize::from(10)))
        );
        assert_eq!(lines.next(), Some(Line::new("bar\n", TextSize::from(4))));
        assert_eq!(
            lines.next_back(),
            Some(Line::new("\r\n", TextSize::from(8)))
        );
        assert_eq!(lines.next(), None);
    }
}
