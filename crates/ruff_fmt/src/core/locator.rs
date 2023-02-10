//! Struct used to efficiently slice source code at (row, column) Locations.

use std::rc::Rc;

use once_cell::unsync::OnceCell;
use rustpython_parser::ast::Location;

use crate::core::types::Range;

pub struct Locator<'a> {
    contents: &'a str,
    contents_rc: Rc<str>,
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
    pub fn new(contents: &'a str) -> Self {
        Locator {
            contents,
            contents_rc: Rc::from(contents),
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

    pub fn slice(&self, range: Range) -> (Rc<str>, usize, usize) {
        let index = self.get_or_init_index();
        let start = truncate(range.location, index, self.contents);
        let end = truncate(range.end_location, index, self.contents);
        (Rc::clone(&self.contents_rc), start, end)
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
}
