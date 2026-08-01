use serde::{Deserialize, Serialize};

use ruff_source_file::{LineColumn, OneIndexed, SourceLocation};

/// Jupyter Notebook indexing table
///
/// When we lint a jupyter notebook, we have to translate the row/column based on
/// [`ruff_text_size::TextSize`] to jupyter notebook cell/row/column.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NotebookIndex {
    /// Stores the starting row and the absolute cell index for every Python (valid) cell.
    ///
    /// The index in this vector corresponds to the Python cell index (valid cell index).
    pub(super) cell_starts: Vec<CellStart>,
}

impl NotebookIndex {
    fn find_cell(&self, row: OneIndexed) -> Option<CellStart> {
        match self
            .cell_starts
            .binary_search_by_key(&row, |start| start.start_row)
        {
            Ok(cell_index) => Some(self.cell_starts[cell_index]),
            Err(insertion_point) => Some(self.cell_starts[insertion_point.checked_sub(1)?]),
        }
    }

    /// Returns the (raw) cell number (1-based) for the given row (1-based).
    pub fn cell(&self, row: OneIndexed) -> Option<OneIndexed> {
        self.find_cell(row).map(|start| start.raw_cell_index)
    }

    /// Returns the row number (1-based) in the cell (1-based) for the
    /// given row (1-based).
    pub fn cell_row(&self, row: OneIndexed) -> Option<OneIndexed> {
        self.find_cell(row)
            .map(|start| OneIndexed::from_zero_indexed(row.get() - start.start_row.get()))
    }

    /// Returns an iterator over the starting rows of each cell (1-based).
    ///
    /// This yields one entry per Python cell (skipping over Markdown cell).
    pub fn iter(&self) -> impl Iterator<Item = CellStart> + '_ {
        self.cell_starts.iter().copied()
    }

    /// Translates the given [`LineColumn`] based on the indexing table.
    ///
    /// This will translate the row/column in the concatenated source code
    /// to the row/column in the Jupyter Notebook cell.
    pub fn translate_line_column(&self, source_location: &LineColumn) -> LineColumn {
        LineColumn {
            line: self
                .cell_row(source_location.line)
                .unwrap_or(OneIndexed::MIN),
            column: source_location.column,
        }
    }

    /// Translates the given [`SourceLocation`] based on the indexing table.
    ///
    /// This will translate the line/character in the concatenated source code
    /// to the line/character in the Jupyter Notebook cell.
    pub fn translate_source_location(&self, source_location: &SourceLocation) -> SourceLocation {
        SourceLocation {
            line: self
                .cell_row(source_location.line)
                .unwrap_or(OneIndexed::MIN),
            character_offset: source_location.character_offset,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CellStart {
    /// The row in the concatenated notebook source code at which
    /// this cell starts.
    pub(super) start_row: OneIndexed,

    /// The absolute index of this cell in the notebook.
    pub(super) raw_cell_index: OneIndexed,
}

impl CellStart {
    pub fn start_row(&self) -> OneIndexed {
        self.start_row
    }

    pub fn cell_index(&self) -> OneIndexed {
        self.raw_cell_index
    }
}
