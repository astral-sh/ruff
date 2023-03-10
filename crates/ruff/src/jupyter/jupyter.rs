use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::iter;
use std::path::Path;

use anyhow::{bail, Context, Result};
use log::debug;
use serde::Serialize;
use serde_json::error::Category;

use crate::jupyter::{CellType, JupyterNotebook, SourceValue};

pub const JUPYTER_NOTEBOOK_EXT: &str = "ipynb";

/// Jupyter Notebook indexing table
///
/// When we lint a jupyter notebook, we have to translate the row/column based [Location]
/// to jupyter notebook cell/row/column.
#[derive(Debug)]
pub struct JupyterIndex {
    /// Enter a row (1-based), get back the cell (1-based)
    pub row_to_cell: Vec<usize>,
    /// Enter a row (1-based), get back the cell (1-based)
    pub row_to_row_in_cell: Vec<usize>,
}

/// Return `true` if the [`Path`] appears to be that of a jupyter notebook file (`.ipynb`).
pub fn is_jupyter_notebook(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == JUPYTER_NOTEBOOK_EXT)
}

pub fn read_jupyter_notebook(path: &Path) -> Result<Option<JupyterNotebook>> {
    let notebook: JupyterNotebook = match serde_json::from_reader(BufReader::new(File::open(path)?))
    {
        Ok(notebook) => notebook,
        Err(err) => match err.classify() {
            Category::Io => return Err(err.into()),
            Category::Syntax | Category::Eof => {
                // Maybe someone saved the python sources (those with the `# %%` separator)
                // as jupyter notebook instead. Let's help them.
                let contents = std::fs::read_to_string(path)?;
                // Check if tokenizing was successful and the file is non-empty
                if !(ruff_rustpython::tokenize(&contents))
                    .last()
                    .map_or(true, |last| last.is_err())
                {
                    return Err(err).context(format!(
                        "Expected a jupyter notebook (.{} extension), \
                        which must be internally stored as json, \
                        but found a python source file",
                        JUPYTER_NOTEBOOK_EXT
                    ));
                }

                return Err(err).context(format!(
                    "A jupyter notebook (.{}) must internally be json, \
                    but this file isn't valid json",
                    JUPYTER_NOTEBOOK_EXT
                ));
            }
            Category::Data => {
                // We could try to read the schema version here but if this fails it's
                // a bug anyway
                return Err(err).context(format!(
                    "This file does not match the schema expected of jupyter notebooks"
                ));
            }
        },
    };

    if notebook.nbformat != 4 {
        // bail because we should have already failed at the json schema stage
        bail!(
            "Expected jupyter notebook format 4, found {}",
            notebook.nbformat
        )
    }
    if !notebook
        .metadata
        .language_info
        .as_ref()
        .map_or(true, |language| language.name == "python")
    {
        debug!(
            "Skipping {} because it's not a python notebook",
            path.display()
        );
        return Ok(None);
    }

    Ok(Some(notebook))
}

/// Concatenates all cells into a single virtual file and builds an index that maps the content
/// to notebook cell locations
pub fn concat_notebook(notebook: JupyterNotebook) -> (String, JupyterIndex) {
    let mut jupyter_index = JupyterIndex {
        // Enter a line number (1-based), get back the cell (1-based)
        // 0 index is just padding
        row_to_cell: vec![0],
        // Enter a line number (1-based), get back the row number in the cell (1-based)
        // 0 index is just padding
        row_to_row_in_cell: vec![0],
    };
    let mut contents = Vec::new();

    for (pos, cell) in notebook
        .cells
        .iter()
        .enumerate()
        .filter(|(_pos, cell)| cell.cell_type == CellType::Code)
    {
        let cell_contents = match &cell.source {
            SourceValue::String(string) => {
                // TODO: is or isn't there a trailing newline per cell?
                // i've only seen these as array and never as string
                let line_count = string.lines().count();
                jupyter_index
                    .row_to_cell
                    .extend(iter::repeat(pos + 1).take(line_count));
                jupyter_index.row_to_row_in_cell.extend(1..line_count + 1);
                string.clone()
            }
            SourceValue::StringArray(string_array) => {
                jupyter_index
                    .row_to_cell
                    .extend(iter::repeat(pos + 1).take(string_array.len()));
                jupyter_index
                    .row_to_row_in_cell
                    .extend(1..string_array.len() + 1);
                // lines already end in a newline character
                string_array.join("")
            }
        };
        contents.push(cell_contents);
    }
    // The last line doesn't end in a newline character
    (contents.join("\n"), jupyter_index)
}

/// Write back with an indent of 1, just like black
pub fn write_jupyter_notebook(path: &Path, notebook: &JupyterNotebook) -> Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    // https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#LL1041
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut writer, formatter);
    notebook.serialize(&mut ser)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use anyhow::Result;

    use super::{is_jupyter_notebook, read_jupyter_notebook};

    #[test]
    fn test_valid() -> Result<()> {
        let path = Path::new("resources/test/fixtures/jupyter/valid.ipynb");
        assert!(read_jupyter_notebook(path)?.is_some());
        Ok(())
    }

    #[test]
    fn test_r() -> Result<()> {
        let path = Path::new("resources/test/fixtures/jupyter/R.ipynb");
        assert!(read_jupyter_notebook(path)?.is_none());
        Ok(())
    }

    #[test]
    fn test_invalid() -> Result<()> {
        let path = Path::new("resources/test/fixtures/jupyter/invalid_extension.ipynb");
        assert_eq!(
            read_jupyter_notebook(path).unwrap_err().to_string(),
            "Expected a jupyter notebook (.ipynb extension), \
            which must be internally stored as json, \
            but found a python source file"
        );
        let path = Path::new("resources/test/fixtures/jupyter/not_json.ipynb");
        assert_eq!(
            read_jupyter_notebook(path).unwrap_err().to_string(),
            "A jupyter notebook (.ipynb) must internally be json, but this file isn't valid json"
        );
        let path = Path::new("resources/test/fixtures/jupyter/wrong_schema.ipynb");
        assert_eq!(
            read_jupyter_notebook(path).unwrap_err().to_string(),
            "This file does not match the schema expected of jupyter notebooks"
        );
        Ok(())
    }

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz");
        assert!(!is_jupyter_notebook(path));

        let path = Path::new("foo/bar/baz.ipynb");
        assert!(!is_jupyter_notebook(path));
    }
}
