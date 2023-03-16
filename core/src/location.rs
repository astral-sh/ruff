#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Source code location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Location {
    pub(super) row: u32,
    pub(super) column: u32,
}

impl Default for Location {
    fn default() -> Self {
        Self { row: 1, column: 0 }
    }
}

impl Location {
    pub fn fmt_with(
        &self,
        f: &mut std::fmt::Formatter,
        e: &impl std::fmt::Display,
    ) -> std::fmt::Result {
        write!(f, "{} at line {} column {}", e, self.row(), self.column())
    }
}

impl Location {
    /// Creates a new Location object at the given row and column.
    ///
    /// # Example
    /// ```
    /// use rustpython_compiler_core::Location;
    /// let loc = Location::new(10, 10);
    /// ```
    pub fn new(row: usize, column: usize) -> Self {
        let row = row.try_into().expect("Location::row over u32");
        let column = column.try_into().expect("Location::column over u32");
        Location { row, column }
    }

    /// Current row
    pub fn row(&self) -> usize {
        self.row as usize
    }

    /// Current column
    pub fn column(&self) -> usize {
        self.column as usize
    }

    pub fn reset(&mut self) {
        self.row = 1;
        self.column = 0;
    }

    pub fn go_right(&mut self) {
        self.column += 1;
    }

    pub fn go_left(&mut self) {
        self.column -= 1;
    }

    pub fn newline(&mut self) {
        self.row += 1;
        self.column = 0;
    }

    pub fn with_col_offset<T: TryInto<isize>>(&self, offset: T) -> Self
    where
        <T as TryInto<isize>>::Error: std::fmt::Debug,
    {
        let column = (self.column as isize
            + offset
                .try_into()
                .expect("offset should be able to convert to isize")) as u32;
        Self {
            row: self.row,
            column,
        }
    }

    pub fn with_row_offset<T: TryInto<isize>>(&self, offset: T) -> Self
    where
        <T as TryInto<isize>>::Error: std::fmt::Debug,
    {
        let row = (self.row as isize
            + offset
                .try_into()
                .expect("offset should be able to convert to isize")) as u32;
        Self {
            row,
            column: self.column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gt() {
        assert!(Location::new(1, 2) > Location::new(1, 1));
        assert!(Location::new(2, 1) > Location::new(1, 1));
        assert!(Location::new(2, 1) > Location::new(1, 2));
    }

    #[test]
    fn test_lt() {
        assert!(Location::new(1, 1) < Location::new(1, 2));
        assert!(Location::new(1, 1) < Location::new(2, 1));
        assert!(Location::new(1, 2) < Location::new(2, 1));
    }

    #[test]
    fn test_with_col_offset() {
        assert_eq!(Location::new(1, 1).with_col_offset(1), Location::new(1, 2));
        assert_eq!(Location::new(1, 1).with_col_offset(-1), Location::new(1, 0));
    }

    #[test]
    fn test_with_row_offset() {
        assert_eq!(Location::new(1, 1).with_row_offset(1), Location::new(2, 1));
        assert_eq!(Location::new(1, 1).with_row_offset(-1), Location::new(0, 1));
    }
}
