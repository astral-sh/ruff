use rustpython_parser::ast::{Located, Location};

use crate::source_code::Locator;
use crate::types::Range;

/// Extract the leading indentation from a line.
pub fn indentation<'a, T>(locator: &'a Locator, located: &'a Located<T>) -> Option<&'a str> {
    let range = Range::from(located);
    let indentation = locator.slice(Range::new(
        Location::new(range.location.row(), 0),
        Location::new(range.location.row(), range.location.column()),
    ));
    if indentation.chars().all(char::is_whitespace) {
        Some(indentation)
    } else {
        None
    }
}

/// Extract the leading words from a line of text.
pub fn leading_words(line: &str) -> &str {
    let line = line.trim();
    line.find(|char: char| !char.is_alphanumeric() && !char.is_whitespace())
        .map_or(line, |index| &line[..index])
}

/// Extract the leading whitespace from a line of text.
pub fn leading_space(line: &str) -> &str {
    line.find(|char: char| !char.is_whitespace())
        .map_or(line, |index| &line[..index])
}

/// Replace any non-whitespace characters from an indentation string.
pub fn clean(indentation: &str) -> String {
    indentation
        .chars()
        .map(|char| if char.is_whitespace() { char } else { ' ' })
        .collect()
}

/// Like `UniversalNewlineIterator`, but includes a trailing newline as an empty line.
pub struct NewlineWithTrailingNewline<'a> {
    trailing: Option<&'a str>,
    underlying: UniversalNewlineIterator<'a>,
}

impl<'a> NewlineWithTrailingNewline<'a> {
    pub fn from(input: &'a str) -> NewlineWithTrailingNewline<'a> {
        NewlineWithTrailingNewline {
            underlying: UniversalNewlineIterator::from(input),
            trailing: if input.ends_with(['\r', '\n']) {
                Some("")
            } else {
                None
            },
        }
    }
}

impl<'a> Iterator for NewlineWithTrailingNewline<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        let mut next = self.underlying.next();
        if next.is_none() {
            if self.trailing.is_some() {
                next = self.trailing;
                self.trailing = None;
            }
        }
        next
    }
}

/// Like `str#lines`, but accommodates LF, CRLF, and CR line endings,
/// the latter of which are not supported by `str#lines`.
pub struct UniversalNewlineIterator<'a> {
    text: &'a str,
    forward_position: usize,
    backwards_position: usize,
}

impl<'a> UniversalNewlineIterator<'a> {
    pub fn from(text: &'a str) -> UniversalNewlineIterator<'a> {
        UniversalNewlineIterator {
            text,
            forward_position: 0,
            backwards_position: text.len(),
        }
    }
}

impl<'a> Iterator for UniversalNewlineIterator<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        if self.forward_position == self.text.len() {
            return None;
        }

        let mut next_pos = None;
        let mut line_end = None;

        for (i, c) in self.text[self.forward_position..].char_indices() {
            match c {
                '\r' => {
                    if let Some('\n') = self.text.chars().nth(i + self.forward_position + 1) {
                        next_pos = Some(i + self.forward_position + 2);
                        line_end = Some(i + self.forward_position);
                    } else {
                        next_pos = Some(i + self.forward_position + 1);
                        line_end = Some(i + self.forward_position);
                    }
                    break;
                }
                '\n' => {
                    next_pos = Some(i + self.forward_position + 1);
                    line_end = Some(i + self.forward_position);
                    break;
                }
                _ => {}
            }
        }

        if let Some(line_end_pos) = line_end {
            let line = &self.text[self.forward_position..line_end_pos];
            self.forward_position = next_pos.unwrap_or(line_end_pos);
            Some(line)
        } else {
            let line = &self.text[self.forward_position..];
            self.forward_position = self.text.len();
            Some(line)
        }
    }
}

impl<'a> DoubleEndedIterator for UniversalNewlineIterator<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.backwards_position == 0 {
            return None;
        }

        // Like `str#lines`, we want to ignore a trailing newline.
        if self.backwards_position == self.text.len() {
            if self.text.ends_with("\r\n") {
                self.backwards_position -= 2;
            } else if self.text.ends_with(['\r', '\n']) {
                self.backwards_position -= 1;
            }
        }

        let mut next_pos = None;
        let mut line_start = None;

        for (i, c) in self.text[..self.backwards_position].char_indices().rev() {
            match c {
                '\r' => {
                    if let Some('\n') = self.text.chars().nth(i - 1) {
                        next_pos = Some(i - 1);
                        line_start = Some(i + 1);
                    } else {
                        next_pos = Some(i);
                        line_start = Some(i + 1);
                    }
                    break;
                }
                '\n' => {
                    next_pos = Some(i);
                    line_start = Some(i + 1);
                    break;
                }
                _ => {}
            }
        }

        if let Some(line_start_pos) = line_start {
            let line = &self.text[line_start_pos..self.backwards_position];
            self.backwards_position = next_pos.unwrap_or(line_start_pos);
            Some(line)
        } else {
            let line = &self.text[..self.backwards_position];
            self.backwards_position = 0;
            Some(line)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UniversalNewlineIterator;

    #[test]
    fn universal_newlines_forward() {
        let text = "foo\nbar\n\r\nbaz\rbop";
        let mut lines = UniversalNewlineIterator::from(text);

        assert_eq!(Some("foo"), lines.next());
        assert_eq!(Some("bar"), lines.next());
        assert_eq!(Some(""), lines.next());
        assert_eq!(Some("baz"), lines.next());
        assert_eq!(Some("bop"), lines.next());

        assert_eq!(None, lines.next());

        let text = "foo\nbar\n\r\nbaz\rbop\n";
        let mut lines = UniversalNewlineIterator::from(text);

        assert_eq!(Some("foo"), lines.next());
        assert_eq!(Some("bar"), lines.next());
        assert_eq!(Some(""), lines.next());
        assert_eq!(Some("baz"), lines.next());
        assert_eq!(Some("bop"), lines.next());

        assert_eq!(None, lines.next());

        let text = "foo\nbar\n\r\nbaz\rbop\n\n";
        let mut lines = UniversalNewlineIterator::from(text);

        assert_eq!(Some("foo"), lines.next());
        assert_eq!(Some("bar"), lines.next());
        assert_eq!(Some(""), lines.next());
        assert_eq!(Some("baz"), lines.next());
        assert_eq!(Some("bop"), lines.next());
        assert_eq!(Some(""), lines.next());

        assert_eq!(None, lines.next());
    }

    #[test]
    fn universal_newlines_backwards() {
        let text = "foo\nbar\n\r\nbaz\rbop";
        let mut lines = UniversalNewlineIterator::from(text).rev();

        assert_eq!(Some("bop"), lines.next());
        assert_eq!(Some("baz"), lines.next());
        assert_eq!(Some(""), lines.next());
        assert_eq!(Some("bar"), lines.next());
        assert_eq!(Some("foo"), lines.next());

        assert_eq!(None, lines.next());

        let text = "foo\nbar\n\r\nbaz\rbop\n";
        let mut lines = UniversalNewlineIterator::from(text).rev();

        assert_eq!(Some("bop"), lines.next());
        assert_eq!(Some("baz"), lines.next());
        assert_eq!(Some(""), lines.next());
        assert_eq!(Some("bar"), lines.next());
        assert_eq!(Some("foo"), lines.next());

        assert_eq!(None, lines.next());
    }
}
