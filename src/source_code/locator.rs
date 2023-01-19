//! Struct used to efficiently slice source code at (row, column) Locations.

use once_cell::unsync::OnceCell;
use rustpython_ast::Location;

use crate::ast::types::Range;

pub struct Locator<'a> {
    contents: &'a str,
    offsets: OnceCell<Vec<Vec<usize>>>,
}

pub fn compute_offsets(contents: &str) -> Vec<Vec<usize>> {
    let mut offsets = Vec::with_capacity(str_indices::lines_crlf::count_breaks(contents));
    let mut current_row = Vec::with_capacity(88);
    let mut current_byte_offset = 0;
    for char in contents.chars() {
        current_row.push(current_byte_offset);
        // This doesn't properly handle CRLF line endings.
        if char == '\n' {
            offsets.push(current_row);
            current_row = Vec::with_capacity(88);
        }
        current_byte_offset += char.len_utf8();
    }
    offsets.push(current_row);
    offsets
}

fn truncate(location: Location, offsets: &Vec<Vec<usize>>, contents: &str) -> usize {
    if (location.row() - 1 == offsets.len() && location.column() == 0)
        || (location.row() - 1 == offsets.len() - 1
            && location.column() == offsets[location.row() - 1].len())
    {
        contents.len()
    } else {
        offsets[location.row() - 1][location.column()]
    }
}

impl<'a> Locator<'a> {
    pub fn new(contents: &'a str) -> Self {
        Locator {
            contents,
            offsets: OnceCell::new(),
        }
    }

    fn get_or_init_offsets(&self) -> &Vec<Vec<usize>> {
        self.offsets.get_or_init(|| compute_offsets(self.contents))
    }

    pub fn slice_source_code_until(&self, location: Location) -> &'a str {
        let offsets = self.get_or_init_offsets();
        let offset = truncate(location, offsets, self.contents);
        &self.contents[..offset]
    }

    pub fn slice_source_code_at(&self, location: Location) -> &'a str {
        let offsets = self.get_or_init_offsets();
        let offset = truncate(location, offsets, self.contents);
        &self.contents[offset..]
    }

    pub fn slice_source_code_range(&self, range: &Range) -> &'a str {
        let offsets = self.get_or_init_offsets();
        let start = truncate(range.location, offsets, self.contents);
        let end = truncate(range.end_location, offsets, self.contents);
        &self.contents[start..end]
    }

    pub fn partition_source_code_at(
        &self,
        outer: &Range,
        inner: &Range,
    ) -> (&'a str, &'a str, &'a str) {
        let offsets = self.get_or_init_offsets();
        let outer_start = truncate(outer.location, offsets, self.contents);
        let outer_end = truncate(outer.end_location, offsets, self.contents);
        let inner_start = truncate(inner.location, offsets, self.contents);
        let inner_end = truncate(inner.end_location, offsets, self.contents);
        (
            &self.contents[outer_start..inner_start],
            &self.contents[inner_start..inner_end],
            &self.contents[inner_end..outer_end],
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::source_code::Locator;

    #[test]
    fn init() {
        let content = "x = 1";
        let locator = Locator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0], [0, 1, 2, 3, 4]);

        let content = "x = 1\n";
        let locator = Locator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(offsets[1], Vec::<usize>::new());

        let content = "x = 1\ny = 2\nz = x + y\n";
        let locator = Locator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 4);
        assert_eq!(offsets[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(offsets[1], [6, 7, 8, 9, 10, 11]);
        assert_eq!(offsets[2], [12, 13, 14, 15, 16, 17, 18, 19, 20, 21]);
        assert_eq!(offsets[3], Vec::<usize>::new());

        let content = "# \u{4e9c}\nclass Foo:\n    \"\"\".\"\"\"";
        let locator = Locator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], [0, 1, 2, 5]);
        assert_eq!(offsets[1], [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(offsets[2], [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27]);
    }
}
