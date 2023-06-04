use std::iter;

use ruff_newlines::NewlineWithTrailingNewline;

use crate::jupyter::{Cell, SourceValue};

/// Jupyter Notebook indexing table
///
/// When we lint a jupyter notebook, we have to translate the row/column based on
/// [`ruff_text_size::TextSize`] to jupyter notebook cell/row/column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JupyterIndex {
    /// Enter a row (1-based), get back the cell (1-based)
    pub(super) row_to_cell: Vec<u32>,
    /// Enter a row (1-based), get back the row in cell (1-based)
    pub(super) row_to_row_in_cell: Vec<u32>,
}

impl JupyterIndex {
    /// Returns the cell number (1-based) for the given row (1-based).
    pub fn cell(&self, row: usize) -> u32 {
        self.row_to_cell[row]
    }

    /// Returns the row number (1-based) in the cell (1-based) for the
    /// given row (1-based).
    pub fn cell_row(&self, row: usize) -> u32 {
        self.row_to_row_in_cell[row]
    }
}

/// Builder for [`JupyterIndex`].
pub(super) struct JupyterIndexBuilder {
    row_to_cell: Vec<u32>,
    row_to_row_in_cell: Vec<u32>,
}

impl Default for JupyterIndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl JupyterIndexBuilder {
    pub(super) fn new() -> Self {
        Self {
            row_to_cell: vec![0],
            row_to_row_in_cell: vec![0],
        }
    }

    /// Add the given code cell to the index, returning the contents of the cell.
    /// The position of the cell is given by `pos` which is the absolute position
    /// of the cell in the notebook.
    pub(super) fn add_code_cell(&mut self, pos: usize, cell: &Cell) -> String {
        let cell_contents = match &cell.source {
            SourceValue::String(string) => {
                let line_count =
                    u32::try_from(NewlineWithTrailingNewline::from(string).count()).unwrap();
                self.row_to_cell.extend(
                    iter::repeat(u32::try_from(pos + 1).unwrap()).take(line_count as usize),
                );
                self.row_to_row_in_cell.extend(1..=line_count);
                string.clone()
            }
            SourceValue::StringArray(string_array) => {
                let trailing_newline =
                    usize::from(string_array.last().map_or(false, |s| s.ends_with('\n')));
                self.row_to_cell.extend(
                    iter::repeat(u32::try_from(pos + 1).unwrap())
                        .take(string_array.len() + trailing_newline),
                );
                self.row_to_row_in_cell
                    .extend(1..=u32::try_from(string_array.len() + trailing_newline).unwrap());
                // lines already end in a newline character
                string_array.join("")
            }
        };
        cell_contents
    }

    pub(super) fn finish(&self) -> JupyterIndex {
        JupyterIndex {
            row_to_cell: self.row_to_cell.clone(),
            row_to_row_in_cell: self.row_to_row_in_cell.clone(),
        }
    }
}
