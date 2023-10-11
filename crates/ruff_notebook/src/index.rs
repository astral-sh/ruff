use serde::{Deserialize, Serialize};

use ruff_source_file::OneIndexed;

/// Jupyter Notebook indexing table
///
/// When we lint a jupyter notebook, we have to translate the row/column based on
/// [`ruff_text_size::TextSize`] to jupyter notebook cell/row/column.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NotebookIndex {
    /// Enter a row (1-based), get back the cell (1-based)
    pub(super) row_to_cell: Vec<OneIndexed>,
    /// Enter a row (1-based), get back the row in cell (1-based)
    pub(super) row_to_row_in_cell: Vec<OneIndexed>,
}

impl NotebookIndex {
    /// Returns the cell number (1-based) for the given row (1-based).
    pub fn cell(&self, row: OneIndexed) -> Option<OneIndexed> {
        self.row_to_cell.get(row.to_zero_indexed()).copied()
    }

    /// Returns the row number (1-based) in the cell (1-based) for the
    /// given row (1-based).
    pub fn cell_row(&self, row: OneIndexed) -> Option<OneIndexed> {
        self.row_to_row_in_cell.get(row.to_zero_indexed()).copied()
    }
}
