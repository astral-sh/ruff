use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::iter;
use std::path::Path;

use log::debug;
use serde::Serialize;
use serde_json::error::Category;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::types::Range;

use crate::jupyter::{CellType, JupyterNotebook, SourceValue};
use crate::rules::pycodestyle::rules::SyntaxError;
use crate::IOError;

pub const JUPYTER_NOTEBOOK_EXT: &str = "ipynb";

/// Jupyter Notebook indexing table
///
/// When we lint a jupyter notebook, we have to translate the row/column based [Location]
/// to jupyter notebook cell/row/column.
#[derive(Debug, Eq, PartialEq)]
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
        // For now this is feature gated here, the long term solution depends on
        // https://github.com/charliermarsh/ruff/issues/3410
        && cfg!(feature = "jupyter_notebook")
}

pub fn read_jupyter_notebook(path: &Path) -> Result<Option<JupyterNotebook>, Box<Diagnostic>> {
    let reader = BufReader::new(File::open(path).map_err(|err| {
        Diagnostic::new(
            IOError {
                message: format!("{err}"),
            },
            Range::default(),
        )
    })?);
    let notebook: JupyterNotebook = match serde_json::from_reader(reader) {
        Ok(notebook) => notebook,
        Err(err) => {
            // Translate the error into a diagnostic
            return Err(Box::new({
                match err.classify() {
                    Category::Io => Diagnostic::new(
                        IOError {
                            message: format!("{err}"),
                        },
                        Range::default(),
                    ),
                    Category::Syntax | Category::Eof => {
                        // Maybe someone saved the python sources (those with the `# %%` separator)
                        // as jupyter notebook instead. Let's help them.
                        let contents = std::fs::read_to_string(path).map_err(|err| {
                            Diagnostic::new(
                                IOError {
                                    message: format!("{err}"),
                                },
                                Range::default(),
                            )
                        })?;
                        // Check if tokenizing was successful and the file is non-empty
                        if (ruff_rustpython::tokenize(&contents))
                            .last()
                            .map_or(true, std::result::Result::is_err)
                        {
                            Diagnostic::new(
                                SyntaxError {
                                    message: format!(
                                        "A Jupyter Notebook (.{JUPYTER_NOTEBOOK_EXT}) must internally be JSON, \
                                but this file isn't valid JSON: {err}"
                                    ),
                                },
                                Range::default(),
                            )
                        } else {
                            Diagnostic::new(
                                SyntaxError {
                                    message: format!(
                                        "Expected a Jupyter Notebook (.{JUPYTER_NOTEBOOK_EXT} extension), \
                                    which must be internally stored as JSON, \
                                    but found a Python source file: {err}"
                                    ),
                                },
                                Range::default(),
                            )
                        }
                    }
                    Category::Data => {
                        // We could try to read the schema version here but if this fails it's
                        // a bug anyway
                        Diagnostic::new(
                            SyntaxError {
                                message: format!(
                                    "This file does not match the schema expected of Jupyter Notebooks: {err}"
                                ),
                            },
                            Range::default(),
                        )
                    }
                }
            }));
        }
    };

    // v4 is what everybody uses
    if notebook.nbformat != 4 {
        // bail because we should have already failed at the json schema stage
        return Err(Box::new(Diagnostic::new(
            SyntaxError {
                message: format!(
                    "Expected Jupyter Notebook format 4, found {}",
                    notebook.nbformat
                ),
            },
            Range::default(),
        )));
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
pub fn concat_notebook(notebook: &JupyterNotebook) -> (String, JupyterIndex) {
    let mut jupyter_index = JupyterIndex {
        // Enter a line number (1-based), get back the cell (1-based)
        // 0 index is just padding
        row_to_cell: vec![0],
        // Enter a line number (1-based), get back the row number in the cell (1-based)
        // 0 index is just padding
        row_to_row_in_cell: vec![0],
    };
    let size_hint = notebook
        .cells
        .iter()
        .filter(|cell| cell.cell_type == CellType::Code)
        .count();

    let mut contents = Vec::with_capacity(size_hint);

    for (pos, cell) in notebook
        .cells
        .iter()
        .enumerate()
        .filter(|(_pos, cell)| cell.cell_type == CellType::Code)
    {
        let cell_contents = match &cell.source {
            SourceValue::String(string) => {
                // TODO(konstin): is or isn't there a trailing newline per cell?
                // i've only seen these as array and never as string
                let line_count = string.lines().count();
                jupyter_index
                    .row_to_cell
                    .extend(iter::repeat(pos + 1).take(line_count));
                jupyter_index.row_to_row_in_cell.extend(1..=line_count);
                string.clone()
            }
            SourceValue::StringArray(string_array) => {
                jupyter_index
                    .row_to_cell
                    .extend(iter::repeat(pos + 1).take(string_array.len()));
                jupyter_index
                    .row_to_row_in_cell
                    .extend(1..=string_array.len());
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
pub fn write_jupyter_notebook(path: &Path, notebook: &JupyterNotebook) -> anyhow::Result<()> {
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

    use crate::jupyter::JupyterIndex;

    use super::{concat_notebook, is_jupyter_notebook, read_jupyter_notebook};

    #[test]
    fn test_valid() {
        let path = Path::new("resources/test/fixtures/jupyter/valid.ipynb");
        assert!(read_jupyter_notebook(path).unwrap().is_some());
    }

    #[test]
    fn test_r() {
        let path = Path::new("resources/test/fixtures/jupyter/R.ipynb");
        assert!(read_jupyter_notebook(path).unwrap().is_none());
    }

    #[test]
    fn test_invalid() {
        let path = Path::new("resources/test/fixtures/jupyter/invalid_extension.ipynb");
        assert_eq!(
            read_jupyter_notebook(path).unwrap_err().kind.body,
            "SyntaxError: Expected a Jupyter Notebook (.ipynb extension), \
            which must be internally stored as JSON, \
            but found a Python source file: \
            expected value at line 1 column 1"
        );
        let path = Path::new("resources/test/fixtures/jupyter/not_json.ipynb");
        assert_eq!(
            read_jupyter_notebook(path).unwrap_err().kind.body,
            "SyntaxError: A Jupyter Notebook (.ipynb) must internally be JSON, \
            but this file isn't valid JSON: \
            expected value at line 1 column 1"
        );
        let path = Path::new("resources/test/fixtures/jupyter/wrong_schema.ipynb");
        assert_eq!(
            read_jupyter_notebook(path).unwrap_err().kind.body,
            "SyntaxError: This file does not match the schema expected of Jupyter Notebooks: \
            missing field `cells` at line 1 column 2"
        );
    }

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz");
        assert!(!is_jupyter_notebook(path));

        let path = Path::new("foo/bar/baz.ipynb");
        assert!(!is_jupyter_notebook(path));
    }

    #[test]
    fn test_concat_notebook() {
        let path = Path::new("resources/test/fixtures/jupyter/valid.ipynb");
        let notebook = read_jupyter_notebook(path).unwrap().unwrap();
        let (contents, index) = concat_notebook(&notebook);
        assert_eq!(
            contents,
            r#"def unused_variable():
    x = 1
    y = 2
    print(f"cell one: {y}")

unused_variable()
def mutable_argument(z=set()):
  print(f"cell two: {z}")

mutable_argument()
"#
        );
        assert_eq!(
            index,
            JupyterIndex {
                row_to_cell: vec![0, 1, 1, 1, 1, 1, 1, 3, 3, 3, 3],
                row_to_row_in_cell: vec![0, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4],
            }
        );
    }
}
