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
        self.index.get_or_init(|| Index::from_str(self.contents))
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

    pub const fn len(&self) -> usize {
        self.contents.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}

/// Index for fast [Location] to byte offset conversions.
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

    /// Builds the index for `content`
    // Not an issue because of manual string length check
    #[allow(clippy::cast_possible_truncation)]
    fn from_str(content: &str) -> Self {
        assert!(u32::try_from(content.len()).is_ok());

        let mut line_start_offsets: Vec<u32> = Vec::with_capacity(48);
        line_start_offsets.push(0);

        for (i, byte) in content.bytes().enumerate() {
            if !byte.is_ascii() {
                return Index::Utf8(continue_non_ascii_content(
                    &content[i..],
                    i as u32,
                    line_start_offsets,
                ));
            }
            if byte == b'\n' {
                line_start_offsets.push((i + 1) as u32);
            }

            continue;
        }

        Self::Ascii(AsciiIndex::new(line_start_offsets))
    }
}

impl From<&str> for Index {
    fn from(value: &str) -> Self {
        Self::from_str(value)
    }
}

/// Index for fast [Location] to byte offset conversions for ASCII documents.
///
/// The index stores the byte offsets for every line. It computes the byte offset for a [Location]
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

// Not an issue because of manual string length check in `Index::from_str`
#[allow(clippy::cast_possible_truncation)]
fn continue_non_ascii_content(non_ascii: &str, mut offset: u32, mut lines: Vec<u32>) -> Utf8Index {
    // Chars up to this point map 1:1 to byte offsets.
    let mut chars_to_byte_offsets = Vec::new();
    chars_to_byte_offsets.extend(0..offset);
    let mut char_index = offset;

    // SKIP BOM
    let contents = if offset == 0 && non_ascii.starts_with('\u{feff}') {
        offset += '\u{feff}'.len_utf8() as u32;
        &non_ascii[offset as usize..]
    } else {
        non_ascii
    };

    let mut after_carriage_return = false;

    for char in contents.chars() {
        match char {
            // Normalize `\r\n` to `\n`
            '\n' if after_carriage_return => continue,
            '\r' | '\n' => {
                lines.push(char_index + 1);
            }
            _ => {}
        }

        chars_to_byte_offsets.push(offset);
        after_carriage_return = char == '\r';
        offset += char.len_utf8() as u32;
        char_index += 1;
    }

    Utf8Index::new(lines, chars_to_byte_offsets)
}

/// Index for fast [Location] to byte offset conversions for UTF8 documents.
///
/// The index stores two lookup tables:
/// * the character offsets for each line
/// * the byte offset for each character
///
/// The byte offset of a [Location] can then be computed using
///
/// ```ignore
/// // retrieving the start character on that line and add the column (character offset)
/// let char_offset = lines[location.row() - 1] + location.column();
/// let byte_offset = char_to_byte_offsets[char_offset]
/// ```
#[derive(Debug, Clone, PartialEq)]
struct Utf8Index {
    /// The index is the line number in the document. The value the character at which the the line starts
    lines_to_characters: Vec<u32>,

    /// The index is the nth character in the document, the value the byte offset from the begining of the document.
    character_to_byte_offsets: Vec<u32>,
}

impl Utf8Index {
    fn new(lines: Vec<u32>, characters: Vec<u32>) -> Self {
        Self {
            lines_to_characters: lines,
            character_to_byte_offsets: characters,
        }
    }

    /// Truncate a [`Location`] to a byte offset in UTF-8 source code.
    fn byte_offset(&self, location: Location, contents: &str) -> usize {
        if location.row() - 1 == self.lines_to_characters.len() && location.column() == 0 {
            contents.len()
        } else {
            let line_start = self.lines_to_characters[location.row() - 1];

            match self
                .character_to_byte_offsets
                .get(line_start as usize + location.column())
            {
                Some(offset) => *offset as usize,
                None => contents.len(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::source_code::locator::{AsciiIndex, Index, Utf8Index};
    use rustpython_parser::ast::Location;

    fn index_ascii(content: &str) -> AsciiIndex {
        match Index::from_str(content) {
            Index::Ascii(ascii) => ascii,
            Index::Utf8(_) => panic!("Expected ASCII index"),
        }
    }

    fn index_utf8(content: &str) -> Utf8Index {
        match Index::from_str(content) {
            Index::Utf8(utf8) => utf8,
            Index::Ascii(_) => panic!("Expected UTF8 Index"),
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

        let contents = "x = 1\r\n";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0, 7]));

        let contents = "x = 1\ny = 2\nz = x + y\n";
        let index = index_ascii(contents);
        assert_eq!(index, AsciiIndex::new(vec![0, 6, 12, 22]));
    }

    #[test]
    fn ascii_truncate() {
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

    impl Utf8Index {
        fn line_count(&self) -> usize {
            self.lines_to_characters.len()
        }
    }

    #[test]
    fn utf8_index() {
        let contents = "x = '🫣'";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 1);
        assert_eq!(index, Utf8Index::new(vec![0], vec![0, 1, 2, 3, 4, 5, 9]));

        let contents = "x = '🫣'\n";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index,
            Utf8Index::new(vec![0, 8], vec![0, 1, 2, 3, 4, 5, 9, 10])
        );

        let contents = "x = '🫣'\r\n";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index,
            Utf8Index::new(vec![0, 8], vec![0, 1, 2, 3, 4, 5, 9, 10])
        );

        let contents = "x = '🫣'\ny = 2\nz = x + y\n";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 4);
        assert_eq!(
            index,
            Utf8Index::new(
                vec![0, 8, 14, 24],
                vec![
                    0, 1, 2, 3, 4, 5, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
                    24, 25, 26
                ]
            )
        );

        let contents = "# 🫣\nclass Foo:\n    \"\"\".\"\"\"";
        let index = index_utf8(contents);
        assert_eq!(index.line_count(), 3);
        assert_eq!(
            index,
            Utf8Index::new(
                vec![0, 4, 15],
                vec![
                    0, 1, 2, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
                    24, 25, 26, 27, 28,
                ]
            )
        );
    }

    #[test]
    fn utf8_byte_offset() {
        let contents = "x = '☃'\ny = 2";
        let index = index_utf8(contents);

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
