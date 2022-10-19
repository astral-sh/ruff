use serde::{Deserialize, Serialize};

/// Sourcecode location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Location {
    pub(super) row: u32,
    pub(super) column: u32,
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
}

#[cfg(test)]
mod tests {
    use crate::Location;

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
}
