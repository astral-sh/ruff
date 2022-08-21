use serde::{Deserialize, Serialize};

/// Sourcecode location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub(super) row: u32,
    pub(super) column: u32,
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
        self.column = 1;
    }

    pub fn go_right(&mut self) {
        self.column += 1;
    }

    pub fn go_left(&mut self) {
        self.column -= 1;
    }

    pub fn newline(&mut self) {
        self.row += 1;
        self.column = 1;
    }
}
