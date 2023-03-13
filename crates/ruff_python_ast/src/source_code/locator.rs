//! Struct used to efficiently slice source code at (row, column) Locations.

use once_cell::unsync::OnceCell;
use rustpython_parser::ast::Location;

use crate::types::Range;

pub struct Locator<'a> {
    contents: &'a str,
    index: OnceCell<Index>,
}

impl<'a> Locator<'a> {
    pub const fn new(contents: &'a str) -> Self {
        Self {
            contents,
            index: OnceCell::new(),
        }
    }

    fn get_or_init_index(&self) -> &Index {
        self.index.get_or_init(|| Index::from(self.contents))
    }

    /// Take the source code up to the given [`Location`].
    pub fn take(&self, location: Location) -> &'a str {
        let index = self.get_or_init_index();
        let offset = index.byte_offset(location, self.contents);
        &self.contents[..offset]
    }

    /// Take the source code after the given [`Location`].
    pub fn skip(&self, location: Location) -> &'a str {
        let index = self.get_or_init_index();
        let offset = index.byte_offset(location, self.contents);
        &self.contents[offset..]
    }

    /// Take the source code between the given [`Range`].
    pub fn slice<R: Into<Range>>(&self, range: R) -> &'a str {
        let index = self.get_or_init_index();
        let range = range.into();
        let start = index.byte_offset(range.location, self.contents);
        let end = index.byte_offset(range.end_location, self.contents);
        &self.contents[start..end]
    }

    /// Return the byte offset of the given [`Location`].
    pub fn offset(&self, location: Location) -> usize {
        let index = self.get_or_init_index();
        index.byte_offset(location, self.contents)
    }

    /// Return the underlying source code.
    pub fn contents(&self) -> &'a str {
        self.contents
    }

    /// Return the number of lines in the source code.
    pub fn count_lines(&self) -> usize {
        let index = self.get_or_init_index();
        index.count_lines()
    }

    /// Return the number of bytes in the source code.
    pub const fn len(&self) -> usize {
        self.contents.len()
    }

    /// Return `true` if the source code is empty.
    pub const fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}

/// Index for fast [`Location`] to byte offset conversions.
#[derive(Debug, Clone)]
enum Index {
    /// Optimized index for an ASCII only document
    Ascii(AsciiIndex),

    /// Index for UTF8 documents
    Utf8(Utf8Index),
}

impl Index {
    /// Truncate a [`Location`] to a byte offset in source code.
    fn byte_offset(&self, location: Location, contents: &str) -> usize {
        match self {
            Index::Ascii(ascii) => ascii.byte_offset(location, contents),
            Index::Utf8(utf8) => utf8.byte_offset(location, contents),
        }
    }

    /// Return the number of lines in the source code.
    fn count_lines(&self) -> usize {
        match self {
            Index::Ascii(ascii) => ascii.line_start_byte_offsets.len(),
            Index::Utf8(utf8) => utf8.line_start_byte_offsets.len(),
        }
    }
}

impl From<&str> for Index {
    fn from(contents: &str) -> Self {
        assert!(u32::try_from(contents.len()).is_ok());

        let mut line_start_offsets: Vec<u32> = Vec::with_capacity(48);
        line_start_offsets.push(0);

        // SAFE because of length assertion above
        #[allow(clippy::cast_possible_truncation)]
        for (i, byte) in contents.bytes().enumerate() {
            if !byte.is_ascii() {
                return Self::Utf8(continue_utf8_index(&contents[i..], i, line_start_offsets));
            }

            match byte {
                // Only track one line break for `\r\n`.
                b'\r' if contents.as_bytes().get(i + 1) == Some(&b'\n') => continue,
                b'\n' | b'\r' => {
                    line_start_offsets.push((i + 1) as u32);
                }
                _ => {}
            }
        }

        Self::Ascii(AsciiIndex::new(line_start_offsets))
    }
}

// SAFE because of length assertion in `Index::from(&str)`
#[allow(clippy::cast_possible_truncation)]
fn continue_utf8_index(
    non_ascii_part: &str,
    offset: usize,
    line_start_offsets: Vec<u32>,
) -> Utf8Index {
    let mut lines = line_start_offsets;

    for (position, char) in non_ascii_part.char_indices() {
        match char {
            // Only track `\n` for `\r\n`
            '\r' if non_ascii_part.as_bytes().get(position + 1) == Some(&b'\n') => continue,
            '\r' | '\n' => {
                let absolute_offset = offset + position + 1;
                lines.push(absolute_offset as u32);
            }
            _ => {}
        }
    }

    Utf8Index::new(lines)
}

/// Index for fast [`Location`] to byte offset conversions for ASCII documents.
///
/// The index stores the byte offsets for every line. It computes the byte offset for a [`Location`]
/// by retrieving the line offset from its index and adding the column.
#[derive(Debug, Clone, Eq, PartialEq)]
struct AsciiIndex {
    line_start_byte_offsets: Vec<u32>,
}

impl AsciiIndex {
    fn new(line_start_positions: Vec<u32>) -> Self {
        Self {
            line_start_byte_offsets: line_start_positions,
        }
    }

    /// Truncate a [`Location`] to a byte offset in ASCII source code.
    fn byte_offset(&self, location: Location, contents: &str) -> usize {
        let index = &self.line_start_byte_offsets;

        // If start-of-line position after last line
        if location.row() - 1 == index.len() && location.column() == 0 {
            contents.len()
        } else {
            let byte_offset = index[location.row() - 1] as usize + location.column();
            byte_offset.min(contents.len())
        }
    }
}

/// Index for fast [`Location`] to byte offset conversions for UTF8 documents.
///
/// The index stores the byte offset of every line. The column offset is lazily computed by
/// adding the line start offset and then iterating to the `nth` character.
#[derive(Debug, Clone, PartialEq)]
struct Utf8Index {
    line_start_byte_offsets: Vec<u32>,
}

impl Utf8Index {
    fn new(line_byte_positions: Vec<u32>) -> Self {
        Self {
            line_start_byte_offsets: line_byte_positions,
        }
    }

    /// Truncate a [`Location`] to a byte offset in UTF-8 source code.
    fn byte_offset(&self, location: Location, contents: &str) -> usize {
        let index = &self.line_start_byte_offsets;

        if location.row() - 1 == index.len() && location.column() == 0 {
            contents.len()
        } else {
            // Casting is safe because the length of utf8 characters is always between 1-4
            #[allow(clippy::cast_possible_truncation)]
            let line_start = if location.row() == 1 && contents.starts_with('\u{feff}') {
                '\u{feff}'.len_utf8() as u32
            } else {
                index[location.row() - 1]
            };

            let rest = &contents[line_start as usize..];

            let column_offset = match rest.char_indices().nth(location.column()) {
                Some((offset, _)) => offset,
                None => contents.len(),
            };

            let offset = line_start as usize + column_offset;
            offset.min(contents.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::source_code::locator::{AsciiIndex, Index, Utf8Index};
    use rustpython_parser::ast::Location;

    fn index_ascii(content: &str) -> AsciiIndex {
        match Index::from(content) {
            Index::Ascii(ascii) => ascii,
            Index::Utf8(_) => {
                panic!("Expected ASCII index")
            }
        }
    }

    fn index_utf8(content: &str) -> Utf8Index {
        match Index::from(content) {
            Index::Utf8(utf8) => utf8,
            Index::Ascii(_) => {
                panic!("Expected UTF8 index")
            }
        }
    }

    #[test]
    fn ascii_index() {
        let contents = "";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0]));

        let contents = "x = 1";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0]));

        let contents = "x = 1\n";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0, 6]));

        let contents = "x = 1\ny = 2\nz = x + y\n";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0, 6, 12, 22]));
    }

    #[test]
    fn ascii_byte_offset() {
        let contents = "x = 1\ny = 2";
        let index = index_ascii(contents);

        // First row.
        let loc = index.byte_offset(Location::new(1, 0), contents);
        assert_eq!(loc, 0);

        // Second row.
        let loc = index.byte_offset(Location::new(2, 0), contents);
        assert_eq!(loc, 6);

        // One-past-the-end.
        let loc = index.byte_offset(Location::new(3, 0), contents);
        assert_eq!(loc, 11);
    }

    #[test]
    fn ascii_carriage_return() {
        let contents = "x = 4\ry = 3";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0, 6]));

        assert_eq!(index.byte_offset(Location::new(1, 4), contents), 4);
        assert_eq!(index.byte_offset(Location::new(2, 0), contents), 6);
        assert_eq!(index.byte_offset(Location::new(2, 1), contents), 7);
    }

    #[test]
    fn ascii_carriage_return_newline() {
        let contents = "x = 4\r\ny = 3";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0, 7]));

        assert_eq!(index.byte_offset(Location::new(1, 4), contents), 4);
        assert_eq!(index.byte_offset(Location::new(2, 0), contents), 7);
        assert_eq!(index.byte_offset(Location::new(2, 1), contents), 8);
    }

    impl Utf8Index {
        fn line_count(&self) -> usize {
            self.line_start_byte_offsets.len()
        }
    }

    #[test]
    fn utf8_index() {
        let contents = "x = '🫣'";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 1);
        assert_eq!(index, Utf8Index::new(vec![0]));

        let contents = "x = '🫣'\n";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index, Utf8Index::new(vec![0, 11]));

        let contents = "x = '🫣'\ny = 2\nz = x + y\n";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 4);
        assert_eq!(index, Utf8Index::new(vec![0, 11, 17, 27]));

        let contents = "# 🫣\nclass Foo:\n    \"\"\".\"\"\"";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 3);
        assert_eq!(index, Utf8Index::new(vec![0, 7, 18]));
    }

    #[test]
    fn utf8_carriage_return() {
        let contents = "x = '🫣'\ry = 3";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index, Utf8Index::new(vec![0, 11]));

        // Second '
        assert_eq!(index.byte_offset(Location::new(1, 6), contents), 9);
        assert_eq!(index.byte_offset(Location::new(2, 0), contents), 11);
        assert_eq!(index.byte_offset(Location::new(2, 1), contents), 12);
    }

    #[test]
    fn utf8_carriage_return_newline() {
        let contents = "x = '🫣'\r\ny = 3";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index, Utf8Index::new(vec![0, 12]));

        // Second '
        assert_eq!(index.byte_offset(Location::new(1, 6), contents), 9);
        assert_eq!(index.byte_offset(Location::new(2, 0), contents), 12);
        assert_eq!(index.byte_offset(Location::new(2, 1), contents), 13);
    }

    #[test]
    fn utf8_byte_offset() {
        let contents = "x = '☃'\ny = 2";
        let index = index_utf8(contents);
        assert_eq!(index, Utf8Index::new(vec![0, 10]));

        // First row.
        let loc = index.byte_offset(Location::new(1, 0), contents);
        assert_eq!(loc, 0);

        let loc = index.byte_offset(Location::new(1, 5), contents);
        assert_eq!(loc, 5);
        assert_eq!(&contents[loc..], "☃'\ny = 2");

        let loc = index.byte_offset(Location::new(1, 6), contents);
        assert_eq!(loc, 8);
        assert_eq!(&contents[loc..], "'\ny = 2");

        // Second row.
        let loc = index.byte_offset(Location::new(2, 0), contents);
        assert_eq!(loc, 10);

        // One-past-the-end.
        let loc = index.byte_offset(Location::new(3, 0), contents);
        assert_eq!(loc, 15);
    }
}
