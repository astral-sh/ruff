//! Struct used to efficiently slice source code at (row, column) Locations.

use crate::source_code::line_index::LineIndex;
use once_cell::unsync::OnceCell;
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Location;

use crate::types::Range;

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

    fn get_or_init_index(&self) -> &LineIndex {
        self.index
            .get_or_init(|| LineIndex::from_source_text(self.contents))
    }

    /// Take the source code up to the given [`Location`].
    pub fn take(&self, location: Location) -> &'a str {
        let index = self.get_or_init_index();
        let offset = index.location_offset(location, self.contents);
        &self.contents[TextRange::up_to(offset)]
    }

    /// Take the source code after the given [`Location`].
    pub fn skip(&self, location: Location) -> &'a str {
        let index = self.get_or_init_index();
        let offset = index.location_offset(location, self.contents);
        &self.contents[usize::from(offset)..]
    }

    /// Take the source code between the given [`Range`].
    pub fn slice<R: Into<Range>>(&self, range: R) -> &'a str {
        let index = self.get_or_init_index();
        let range = range.into();
        let start = index.location_offset(range.location, self.contents);
        let end = index.location_offset(range.end_location, self.contents);
        &self.contents[TextRange::new(start, end)]
    }

    /// Return the byte offset of the given [`Location`].
    pub fn offset(&self, location: Location) -> TextSize {
        let index = self.get_or_init_index();
        index.location_offset(location, self.contents)
    }

    /// Return the underlying source code.
    pub fn contents(&self) -> &'a str {
        self.contents
    }

    /// Return the number of lines in the source code.
    pub fn count_lines(&self) -> usize {
        let index = self.get_or_init_index();
        index.lines_count()
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
