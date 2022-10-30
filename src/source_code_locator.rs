//! Struct used to efficiently slice source code at (row, column) Locations.

use once_cell::unsync::OnceCell;
use rustpython_ast::Location;

use crate::ast::types::Range;

pub struct SourceCodeLocator<'a> {
    contents: &'a str,
    offsets: OnceCell<Vec<Vec<usize>>>,
}

pub fn compute_offsets(contents: &str) -> Vec<Vec<usize>> {
    let mut offsets = vec![vec![]];
    let mut line_index = 0;
    let mut char_index = 0;
    let mut newline = false;
    for (i, char) in contents.char_indices() {
        offsets[line_index].push(i);

        newline = char == '\n';
        if newline {
            line_index += 1;
            offsets.push(vec![]);
            char_index = i + char.len_utf8();
        }
    }
    // If we end in a newline, add an extra character to indicate the start of that line.
    if newline {
        offsets[line_index].push(char_index);
    }
    offsets
}

impl<'a> SourceCodeLocator<'a> {
    pub fn new(contents: &'a str) -> Self {
        SourceCodeLocator {
            contents,
            offsets: OnceCell::new(),
        }
    }

    fn get_or_init_offsets(&self) -> &Vec<Vec<usize>> {
        self.offsets.get_or_init(|| compute_offsets(self.contents))
    }

    pub fn slice_source_code_at(&self, location: &Location) -> &'a str {
        let offsets = self.get_or_init_offsets();
        let offset = offsets[location.row() - 1][location.column()];
        &self.contents[offset..]
    }

    pub fn slice_source_code_range(&self, range: &Range) -> &'a str {
        let offsets = self.get_or_init_offsets();
        let start = offsets[range.location.row() - 1][range.location.column()];
        let end = offsets[range.end_location.row() - 1][range.end_location.column()];
        &self.contents[start..end]
    }

    pub fn partition_source_code_at(
        &self,
        outer: &Range,
        inner: &Range,
    ) -> (&'a str, &'a str, &'a str) {
        let offsets = self.get_or_init_offsets();
        let outer_start = offsets[outer.location.row() - 1][outer.location.column()];
        let outer_end = offsets[outer.end_location.row() - 1][outer.end_location.column()];
        let inner_start = offsets[inner.location.row() - 1][inner.location.column()];
        let inner_end = offsets[inner.end_location.row() - 1][inner.end_location.column()];
        (
            &self.contents[outer_start..inner_start],
            &self.contents[inner_start..inner_end],
            &self.contents[inner_end..outer_end],
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::source_code_locator::SourceCodeLocator;

    #[test]
    fn source_code_locator_init() {
        let content = "x = 1";
        let locator = SourceCodeLocator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0], [0, 1, 2, 3, 4]);

        let content = "x = 1\n";
        let locator = SourceCodeLocator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(offsets[1], [6]);

        let content = "x = 1\ny = 2\nz = x + y\n";
        let locator = SourceCodeLocator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 4);
        assert_eq!(offsets[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(offsets[1], [6, 7, 8, 9, 10, 11]);
        assert_eq!(offsets[2], [12, 13, 14, 15, 16, 17, 18, 19, 20, 21]);
        assert_eq!(offsets[3], [22]);

        let content = "# \u{4e9c}\nclass Foo:\n    \"\"\".\"\"\"";
        let locator = SourceCodeLocator::new(content);
        let offsets = locator.get_or_init_offsets();
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], [0, 1, 2, 5]);
        assert_eq!(offsets[1], [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(offsets[2], [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27]);
    }
}
