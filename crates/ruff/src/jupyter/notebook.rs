use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::iter;
use std::path::Path;

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
/// When we lint a jupyter notebook, we have to translate the row/column based on
/// [`crate::message::Location`]
/// to jupyter notebook cell/row/column.
#[derive(Debug, Eq, PartialEq)]
pub struct JupyterIndex {
    /// Enter a row (1-based), get back the cell (1-based)
    pub row_to_cell: Vec<u32>,
    /// Enter a row (1-based), get back the cell (1-based)
    pub row_to_row_in_cell: Vec<u32>,
}

/// Return `true` if the [`Path`] appears to be that of a jupyter notebook file (`.ipynb`).
pub fn is_jupyter_notebook(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == JUPYTER_NOTEBOOK_EXT)
        // For now this is feature gated here, the long term solution depends on
        // https://github.com/charliermarsh/ruff/issues/3410
        && cfg!(feature = "jupyter_notebook")
}

impl JupyterNotebook {
    /// See also the black implementation
    /// <https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#L1017-L1046>
    pub fn read(path: &Path) -> Result<Self, Box<Diagnostic>> {
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
                                .map_or(true, Result::is_err)
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

        Ok(notebook)
    }

    /// Concatenates all cells into a single virtual file and builds an index that maps the content
    /// to notebook cell locations
    pub fn index(&self) -> (String, JupyterIndex) {
        let mut jupyter_index = JupyterIndex {
            // Enter a line number (1-based), get back the cell (1-based)
            // 0 index is just padding
            row_to_cell: vec![0],
            // Enter a line number (1-based), get back the row number in the cell (1-based)
            // 0 index is just padding
            row_to_row_in_cell: vec![0],
        };
        let size_hint = self
            .cells
            .iter()
            .filter(|cell| cell.cell_type == CellType::Code)
            .count();

        let mut contents = Vec::with_capacity(size_hint);

        for (pos, cell) in self
            .cells
            .iter()
            .enumerate()
            .filter(|(_pos, cell)| cell.cell_type == CellType::Code)
        {
            let cell_contents = match &cell.source {
                SourceValue::String(string) => {
                    // TODO(konstin): is or isn't there a trailing newline per cell?
                    // i've only seen these as array and never as string
                    let line_count = u32::try_from(string.lines().count()).unwrap();
                    jupyter_index.row_to_cell.extend(
                        iter::repeat(u32::try_from(pos + 1).unwrap()).take(line_count as usize),
                    );
                    jupyter_index.row_to_row_in_cell.extend(1..=line_count);
                    string.clone()
                }
                SourceValue::StringArray(string_array) => {
                    jupyter_index.row_to_cell.extend(
                        iter::repeat(u32::try_from(pos + 1).unwrap()).take(string_array.len()),
                    );
                    jupyter_index
                        .row_to_row_in_cell
                        .extend(1..=u32::try_from(string_array.len()).unwrap());
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
    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        let mut writer = BufWriter::new(File::create(path)?);
        // https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#LL1041
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut writer, formatter);
        self.serialize(&mut ser)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    #[cfg(feature = "jupyter_notebook")]
    use crate::jupyter::is_jupyter_notebook;
    use crate::jupyter::{JupyterIndex, JupyterNotebook};

    #[test]
    fn test_valid() {
        let path = Path::new("resources/test/fixtures/jupyter/valid.ipynb");
        assert!(JupyterNotebook::read(path).is_ok());
    }

    #[test]
    fn test_r() {
        // We can load this, it will be filtered out later
        let path = Path::new("resources/test/fixtures/jupyter/R.ipynb");
        assert!(JupyterNotebook::read(path).is_ok());
    }

    #[test]
    fn test_invalid() {
        let path = Path::new("resources/test/fixtures/jupyter/invalid_extension.ipynb");
        assert_eq!(
            JupyterNotebook::read(path).unwrap_err().kind.body,
            "SyntaxError: Expected a Jupyter Notebook (.ipynb extension), \
            which must be internally stored as JSON, \
            but found a Python source file: \
            expected value at line 1 column 1"
        );
        let path = Path::new("resources/test/fixtures/jupyter/not_json.ipynb");
        assert_eq!(
            JupyterNotebook::read(path).unwrap_err().kind.body,
            "SyntaxError: A Jupyter Notebook (.ipynb) must internally be JSON, \
            but this file isn't valid JSON: \
            expected value at line 1 column 1"
        );
        let path = Path::new("resources/test/fixtures/jupyter/wrong_schema.ipynb");
        assert_eq!(
            JupyterNotebook::read(path).unwrap_err().kind.body,
            "SyntaxError: This file does not match the schema expected of Jupyter Notebooks: \
            missing field `cells` at line 1 column 2"
        );
    }

    #[test]
    #[cfg(feature = "jupyter_notebook")]
    fn inclusions() {
        let path = Path::new("foo/bar/baz");
        assert!(!is_jupyter_notebook(path));

        let path = Path::new("foo/bar/baz.ipynb");
        assert!(is_jupyter_notebook(path));
    }

    #[test]
    fn test_concat_notebook() {
        let path = Path::new("resources/test/fixtures/jupyter/valid.ipynb");
        let notebook = JupyterNotebook::read(path).unwrap();
        let (contents, index) = notebook.index();
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
