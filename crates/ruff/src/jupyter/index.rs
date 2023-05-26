use std::iter;
use std::sync::Arc;

use crate::jupyter::{Cell, SourceValue};

/// Jupyter Notebook indexing table
///
/// When we lint a jupyter notebook, we have to translate the row/column based on
/// [`ruff_text_size::TextSize`] to jupyter notebook cell/row/column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JupyterIndex {
    inner: Arc<JupyterIndexInner>,
}

impl JupyterIndex {
    pub fn get_cell(&self, row: usize) -> u32 {
        self.inner.row_to_cell[row]
    }

    pub fn get_row_in_cell(&self, row: usize) -> u32 {
        self.inner.row_to_row_in_cell[row]
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct JupyterIndexInner {
    /// Enter a row (1-based), get back the cell (1-based)
    row_to_cell: Vec<u32>,
    /// Enter a row (1-based), get back the cell (1-based)
    row_to_row_in_cell: Vec<u32>,
}

/// Builder for [`JupyterIndex`].
pub struct JupyterIndexBuilder {
    row_to_cell: Vec<u32>,
    row_to_row_in_cell: Vec<u32>,
}

impl Default for JupyterIndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl JupyterIndexBuilder {
    pub fn new() -> Self {
        Self {
            row_to_cell: vec![0],
            row_to_row_in_cell: vec![0],
        }
    }

    pub fn add_cell(&mut self, pos: usize, cell: &Cell) -> String {
        let cell_contents = match &cell.source {
            SourceValue::String(string) => {
                let line_count = u32::try_from(string.lines().count()).unwrap();
                self.row_to_cell.extend(
                    iter::repeat(u32::try_from(pos + 1).unwrap()).take(line_count as usize),
                );
                self.row_to_row_in_cell.extend(1..=line_count);
                string.clone()
            }
            SourceValue::StringArray(string_array) => {
                self.row_to_cell
                    .extend(iter::repeat(u32::try_from(pos + 1).unwrap()).take(string_array.len()));
                self.row_to_row_in_cell
                    .extend(1..=u32::try_from(string_array.len()).unwrap());
                // lines already end in a newline character
                string_array.join("")
            }
        };
        cell_contents
    }

    pub fn finish(&self) -> JupyterIndex {
        JupyterIndex {
            inner: Arc::new(JupyterIndexInner {
                row_to_cell: self.row_to_cell.clone(),
                row_to_row_in_cell: self.row_to_row_in_cell.clone(),
            }),
        }
    }
}
