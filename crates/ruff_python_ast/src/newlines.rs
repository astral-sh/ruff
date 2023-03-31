use ruff_text_size::{TextLen, TextRange, TextSize};
use std::iter::FusedIterator;
use std::ops::Deref;

/// Extension trait for [`str`] that provides a [`UniversalNewlineIterator`].
pub trait StrExt {
    fn universal_newlines(&self) -> UniversalNewlineIterator<'_>;
}

impl StrExt for str {
    fn universal_newlines(&self) -> UniversalNewlineIterator<'_> {
        UniversalNewlineIterator::from(self)
    }
}

/// Like [`str#lines`], but accommodates LF, CRLF, and CR line endings,
/// the latter of which are not supported by [`str#lines`].
///
/// ## Examples
///
/// ```rust
/// # use ruff_text_size::TextSize;
/// # use ruff_python_ast::newlines::{Line, UniversalNewlineIterator};
/// let mut lines = UniversalNewlineIterator::from("foo\nbar\n\r\nbaz\rbop");
///
/// assert_eq!(lines.next_back(), Some(Line::new("bop", TextSize::from(14))));
/// assert_eq!(lines.next(), Some(Line::new("foo\n", TextSize::from(0))));
/// assert_eq!(lines.next_back(), Some(Line::new("baz\r", TextSize::from(10))));
/// assert_eq!(lines.next(), Some(Line::new("bar\n", TextSize::from(4))));
/// assert_eq!(lines.next_back(), Some(Line::new("\r\n", TextSize::from(8))));
/// assert_eq!(lines.next(), None);
/// ```
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

impl<'a> Iterator for UniversalNewlineIterator<'a> {
    type Item = Line<'a>;

    #[inline]
    fn next(&mut self) -> Option<Line<'a>> {
        if self.text.is_empty() {
            return None;
        }

        let line = match self.text.find(['\n', '\r']) {
            // Non-last line
            Some(line_end) => {
                let offset: usize = match self.text.as_bytes()[line_end] {
                    // Explicit branch for `\n` as this is the most likely path
                    b'\n' => 1,
                    // '\r\n'
                    b'\r' if self.text.as_bytes().get(line_end + 1) == Some(&b'\n') => 2,
                    // '\r'
                    _ => 1,
                };

                let (text, remainder) = self.text.split_at(line_end + offset);

                let line = Line {
                    offset: self.offset,
                    text,
                };

                self.text = remainder;
                self.offset += text.text_len();

                line
            }
            // Last line
            None => Line {
                offset: self.offset,
                text: std::mem::take(&mut self.text),
            },
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
        let line = match haystack.rfind(['\n', '\r']) {
            // '\n' or '\r' or '\r\n'
            Some(line_end) => {
                let (remainder, line) = self.text.split_at(line_end + 1);
                self.text = remainder;
                self.offset_back -= line.text_len();

                Line {
                    text: line,
                    offset: self.offset_back,
                }
            }
            // Last line
            None => {
                let offset = self.offset_back - self.text.text_len();
                Line {
                    text: std::mem::take(&mut self.text),
                    offset,
                }
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
    fn next(&mut self) -> Option<Line<'a>> {
        self.underlying.next().or_else(|| self.trailing.take())
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

    /// Returns the text of the line, excluding the terminating new line character.
    #[inline]
    pub fn as_str(&self) -> &'a str {
        let mut bytes = self.text.bytes().rev();

        let newline_len = match bytes.next() {
            Some(b'\n') => {
                if bytes.next() == Some(b'\r') {
                    2
                } else {
                    1
                }
            }
            Some(b'\r') => 1,
            _ => 0,
        };

        &self.text[..self.text.len() - newline_len]
    }

    /// Returns the line's text, including the terminating new line character.
    #[inline]
    pub fn as_full_str(&self) -> &'a str {
        &self.text
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

#[cfg(test)]
mod tests {
    use super::UniversalNewlineIterator;
    use crate::newlines::Line;
    use ruff_text_size::TextSize;

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
        assert_eq!(lines.next(), None)
    }
}
