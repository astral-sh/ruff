//! Struct used to efficiently slice source code at (row, column) Locations.

use once_cell::unsync::OnceCell;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;

pub struct Locator<'a> {
    contents: &'a str,
    index: OnceCell<Index>,
}

pub enum Index {
    Ascii(Vec<usize>),
    Utf8(Vec<Vec<usize>>),
}

/// Compute the starting byte index of each line in ASCII source code.
fn index_ascii(contents: &str) -> Vec<usize> {
    let mut index = Vec::with_capacity(48);
    index.push(0);
    let bytes = contents.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            index.push(i + 1);
        }
    }
    index
}

/// Compute the starting byte index of each character in UTF-8 source code.
fn index_utf8(contents: &str) -> Vec<Vec<usize>> {
    let mut index = Vec::with_capacity(48);
    let mut current_row = Vec::with_capacity(48);
    let mut current_byte_offset = 0;
    let mut previous_char = '\0';
    for char in contents.chars() {
        current_row.push(current_byte_offset);
        if char == '\n' {
            if previous_char == '\r' {
                current_row.pop();
            }
            index.push(current_row);
            current_row = Vec::with_capacity(48);
        }
        current_byte_offset += char.len_utf8();
        previous_char = char;
    }
    index.push(current_row);
    index
}

/// Compute the starting byte index of each line in source code.
pub fn index(contents: &str) -> Index {
    if contents.is_ascii() {
        Index::Ascii(index_ascii(contents))
    } else {
        Index::Utf8(index_utf8(contents))
    }
}

/// Truncate a [`Location`] to a byte offset in ASCII source code.
fn truncate_ascii(location: Location, index: &[usize], contents: &str) -> usize {
    if location.row() - 1 == index.len() && location.column() == 0
        || (!index.is_empty()
            && location.row() - 1 == index.len() - 1
            && index[location.row() - 1] + location.column() >= contents.len())
    {
        contents.len()
    } else {
        index[location.row() - 1] + location.column()
    }
}

/// Truncate a [`Location`] to a byte offset in UTF-8 source code.
fn truncate_utf8(location: Location, index: &[Vec<usize>], contents: &str) -> usize {
    if (location.row() - 1 == index.len() && location.column() == 0)
        || (location.row() - 1 == index.len() - 1
            && location.column() == index[location.row() - 1].len())
    {
        contents.len()
    } else {
        index[location.row() - 1][location.column()]
    }
}

/// Truncate a [`Location`] to a byte offset in source code.
fn truncate(location: Location, index: &Index, contents: &str) -> usize {
    match index {
        Index::Ascii(index) => truncate_ascii(location, index, contents),
        Index::Utf8(index) => truncate_utf8(location, index, contents),
    }
}

impl<'a> Locator<'a> {
    pub const fn new(contents: &'a str) -> Self {
        Self {
            contents,
            index: OnceCell::new(),
        }
    }

    fn get_or_init_index(&self) -> &Index {
        self.index.get_or_init(|| index(self.contents))
    }

    pub fn slice_source_code_until(&self, location: Location) -> &'a str {
        let index = self.get_or_init_index();
        let offset = truncate(location, index, self.contents);
        &self.contents[..offset]
    }

    pub fn slice_source_code_at(&self, location: Location) -> &'a str {
        let index = self.get_or_init_index();
        let offset = truncate(location, index, self.contents);
        &self.contents[offset..]
    }

    pub fn slice_source_code_range(&self, range: &Range) -> &'a str {
        let index = self.get_or_init_index();
        let start = truncate(range.location, index, self.contents);
        let end = truncate(range.end_location, index, self.contents);
        &self.contents[start..end]
    }

    pub fn partition_source_code_at(
        &self,
        outer: &Range,
        inner: &Range,
    ) -> (&'a str, &'a str, &'a str) {
        let index = self.get_or_init_index();
        let outer_start = truncate(outer.location, index, self.contents);
        let outer_end = truncate(outer.end_location, index, self.contents);
        let inner_start = truncate(inner.location, index, self.contents);
        let inner_end = truncate(inner.end_location, index, self.contents);
        (
            &self.contents[outer_start..inner_start],
            &self.contents[inner_start..inner_end],
            &self.contents[inner_end..outer_end],
        )
    }

    pub const fn len(&self) -> usize {
        self.contents.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::Location;

    use crate::source_code::locator::{index_ascii, index_utf8, truncate_ascii, truncate_utf8};

    #[test]
    fn ascii_index() {
        let contents = "";
        let index = index_ascii(contents);
        assert_eq!(index, [0]);

        let contents = "x = 1";
        let index = index_ascii(contents);
        assert_eq!(index, [0]);

        let contents = "x = 1\n";
        let index = index_ascii(contents);
        assert_eq!(index, [0, 6]);

        let contents = "x = 1\r\n";
        let index = index_ascii(contents);
        assert_eq!(index, [0, 7]);

        let contents = "x = 1\ny = 2\nz = x + y\n";
        let index = index_ascii(contents);
        assert_eq!(index, [0, 6, 12, 22]);
    }

    #[test]
    fn ascii_truncate() {
        let contents = "x = 1\ny = 2";
        let index = index_ascii(contents);

        // First row.
        let loc = truncate_ascii(Location::new(1, 0), &index, contents);
        assert_eq!(loc, 0);

        // Second row.
        let loc = truncate_ascii(Location::new(2, 0), &index, contents);
        assert_eq!(loc, 6);

        // One-past-the-end.
        let loc = truncate_ascii(Location::new(3, 0), &index, contents);
        assert_eq!(loc, 11);
    }

    #[test]
    fn utf8_index() {
        let contents = "";
        let index = index_utf8(contents);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0], Vec::<usize>::new());

        let contents = "x = 1";
        let index = index_utf8(contents);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0], [0, 1, 2, 3, 4]);

        let contents = "x = 1\n";
        let index = index_utf8(contents);
        assert_eq!(index.len(), 2);
        assert_eq!(index[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(index[1], Vec::<usize>::new());

        let contents = "x = 1\r\n";
        let index = index_utf8(contents);
        assert_eq!(index.len(), 2);
        assert_eq!(index[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(index[1], Vec::<usize>::new());

        let contents = "x = 1\ny = 2\nz = x + y\n";
        let index = index_utf8(contents);
        assert_eq!(index.len(), 4);
        assert_eq!(index[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(index[1], [6, 7, 8, 9, 10, 11]);
        assert_eq!(index[2], [12, 13, 14, 15, 16, 17, 18, 19, 20, 21]);
        assert_eq!(index[3], Vec::<usize>::new());

        let contents = "# \u{4e9c}\nclass Foo:\n    \"\"\".\"\"\"";
        let index = index_utf8(contents);
        assert_eq!(index.len(), 3);
        assert_eq!(index[0], [0, 1, 2, 5]);
        assert_eq!(index[1], [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(index[2], [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27]);
    }

    #[test]
    fn utf8_truncate() {
        let contents = "x = '☃'\ny = 2";
        let index = index_utf8(contents);

        // First row.
        let loc = truncate_utf8(Location::new(1, 0), &index, contents);
        assert_eq!(loc, 0);

        let loc = truncate_utf8(Location::new(1, 5), &index, contents);
        assert_eq!(loc, 5);
        assert_eq!(&contents[loc..], "☃'\ny = 2");

        let loc = truncate_utf8(Location::new(1, 6), &index, contents);
        assert_eq!(loc, 8);
        assert_eq!(&contents[loc..], "'\ny = 2");

        // Second row.
        let loc = truncate_utf8(Location::new(2, 0), &index, contents);
        assert_eq!(loc, 10);

        // One-past-the-end.
        let loc = truncate_utf8(Location::new(3, 0), &index, contents);
        assert_eq!(loc, 15);
    }
}
