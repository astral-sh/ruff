//! Struct used to efficiently slice source code at (row, column) Locations.

use once_cell::unsync::OnceCell;
use ropey::Rope;
use rustpython_ast::Location;

use crate::ast::types::Range;

pub struct SourceCodeLocator<'a> {
    contents: &'a str,
    rope: OnceCell<Rope>,
}

impl<'a> SourceCodeLocator<'a> {
    pub fn new(contents: &'a str) -> Self {
        SourceCodeLocator {
            contents,
            rope: OnceCell::new(),
        }
    }

    fn get_or_init_rope(&self) -> &Rope {
        self.rope.get_or_init(|| Rope::from_str(self.contents))
    }

    pub fn slice_source_code_at(&self, location: &Location) -> &'a str {
        let rope = self.get_or_init_rope();
        let offset = rope.line_to_char(location.row() - 1) + location.column();
        &self.contents[offset..]
    }

    pub fn slice_source_code_range(&self, range: &Range) -> &'a str {
        let rope = self.get_or_init_rope();
        let start = rope.line_to_char(range.location.row() - 1) + range.location.column();
        let end = rope.line_to_char(range.end_location.row() - 1) + range.end_location.column();
        &self.contents[start..end]
    }

    pub fn partition_source_code_at(
        &self,
        outer: &Range,
        inner: &Range,
    ) -> (&'a str, &'a str, &'a str) {
        let rope = self.get_or_init_rope();
        let outer_start = rope.line_to_char(outer.location.row() - 1) + outer.location.column();
        let outer_end =
            rope.line_to_char(outer.end_location.row() - 1) + outer.end_location.column();
        let inner_start = rope.line_to_char(inner.location.row() - 1) + inner.location.column();
        let inner_end =
            rope.line_to_char(inner.end_location.row() - 1) + inner.end_location.column();
        (
            &self.contents[outer_start..inner_start],
            &self.contents[inner_start..inner_end],
            &self.contents[inner_end..outer_end],
        )
    }
}
