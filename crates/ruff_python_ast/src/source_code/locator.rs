//! Struct used to efficiently slice source code at (row, column) Locations.

use crate::source_code::line_index::LineIndex;
use crate::source_code::SourceCode;
use once_cell::unsync::OnceCell;
use ruff_text_size::TextSize;
use rustpython_parser::ast::Location;

use crate::types::Range;

pub struct Locator<'a> {
    contents: &'a str,
    line_index: OnceCell<LineIndex>,
}

impl<'a> Locator<'a> {
    pub const fn new(contents: &'a str) -> Self {
        Self {
            contents,
            line_index: OnceCell::new(),
        }
    }

    fn get_or_init_index(&self) -> &LineIndex {
        self.line_index
            .get_or_init(|| LineIndex::from_source_text(self.contents))
    }

    #[inline]
    pub fn to_source_code(&self) -> SourceCode<'a, '_> {
        SourceCode {
            index: self.get_or_init_index(),
            text: self.contents,
        }
    }

    /// Take the source code up to the given [`Location`].
    #[inline]
    pub fn up_to(&self, location: Location) -> &'a str {
        self.to_source_code().up_to(location)
    }

    /// Take the source code after the given [`Location`].
    #[inline]
    pub fn after(&self, location: Location) -> &'a str {
        self.to_source_code().after(location)
    }

    /// Take the source code between the given [`Range`].
    #[inline]
    pub fn slice<R: Into<Range>>(&self, range: R) -> &'a str {
        self.to_source_code().slice(range)
    }

    /// Return the byte offset of the given [`Location`].
    #[inline]
    pub fn offset(&self, location: Location) -> TextSize {
        self.to_source_code().offset(location)
    }

    /// Return the underlying source code.
    pub fn contents(&self) -> &'a str {
        self.contents
    }

    /// Return the number of lines in the source code.
    pub fn count_lines(&self) -> usize {
        let index = self.get_or_init_index();
        index.line_count()
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
