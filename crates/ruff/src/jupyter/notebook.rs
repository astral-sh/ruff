use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use itertools::Itertools;
use serde::Serialize;
use serde_json::error::Category;

use ruff_diagnostics::{Diagnostic, Edit};
use ruff_text_size::{TextRange, TextSize};

use crate::jupyter::index::{JupyterIndex, JupyterIndexBuilder};
use crate::jupyter::{CellType, JupyterNotebook, SourceValue};
use crate::rules::pycodestyle::rules::SyntaxError;
use crate::IOError;

pub const JUPYTER_NOTEBOOK_EXT: &str = "ipynb";

/// Return `true` if the [`Path`] appears to be that of a jupyter notebook file (`.ipynb`).
pub fn is_jupyter_notebook(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == JUPYTER_NOTEBOOK_EXT)
        // For now this is feature gated here, the long term solution depends on
        // https://github.com/astral-sh/ruff/issues/3410
        && cfg!(feature = "jupyter_notebook")
}

#[derive(Debug)]
pub struct Notebook {
    pub index: JupyterIndex,
    pub content: String,
    raw: JupyterNotebook,
    cell_offsets: Vec<TextSize>,
}

impl Notebook {
    /// See also the black implementation
    /// <https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#L1017-L1046>
    pub fn read(path: &Path) -> Result<Self, Box<Diagnostic>> {
        let reader = BufReader::new(File::open(path).map_err(|err| {
            Diagnostic::new(
                IOError {
                    message: format!("{err}"),
                },
                TextRange::default(),
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
                            TextRange::default(),
                        ),
                        Category::Syntax | Category::Eof => {
                            // Maybe someone saved the python sources (those with the `# %%` separator)
                            // as jupyter notebook instead. Let's help them.
                            let contents = std::fs::read_to_string(path).map_err(|err| {
                                Diagnostic::new(
                                    IOError {
                                        message: format!("{err}"),
                                    },
                                    TextRange::default(),
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
                                    TextRange::default(),
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
                                    TextRange::default(),
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
                                TextRange::default(),
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
                TextRange::default(),
            )));
        }

        let size_hint = notebook
            .cells
            .iter()
            .filter(|cell| cell.cell_type == CellType::Code)
            .count();

        let mut current_offset = TextSize::from(0);
        let mut cell_offsets = Vec::with_capacity(notebook.cells.len());
        cell_offsets.push(TextSize::from(0));

        let mut contents = Vec::with_capacity(size_hint);
        let mut builder = JupyterIndexBuilder::default();

        for (pos, cell) in notebook
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.cell_type == CellType::Code)
        {
            let cell_contents = builder.add_cell(pos, cell);
            current_offset += TextSize::of(&cell_contents) + TextSize::new(1);
            cell_offsets.push(current_offset);
            contents.push(cell_contents);
        }

        if cell_offsets.len() > 1 {
            // Remove the last newline offset
            *cell_offsets.last_mut().unwrap() -= TextSize::new(1);
        }

        Ok(Self {
            raw: notebook,
            index: builder.finish(),
            content: contents.join("\n"),
            cell_offsets,
        })
    }

    fn update_cell_offsets(&mut self, edits: BTreeSet<&Edit>) {
        for edit in edits.into_iter().rev() {
            let idx = self
                .cell_offsets
                .iter()
                .tuple_windows::<(_, _)>()
                .find_position(|(&offset1, &offset2)| {
                    offset1 <= edit.start() && edit.end() <= offset2
                })
                .map_or_else(
                    || panic!("edit outside of any cells: {edit:?}"),
                    |(idx, _)| idx,
                );

            if edit.is_deletion() {
                for offset in &mut self.cell_offsets[idx + 1..] {
                    *offset -= edit.range().len();
                }
            } else if edit.is_insertion() {
                let new_text_size = TextSize::of(edit.content().unwrap_or_default());
                for offset in &mut self.cell_offsets[idx + 1..] {
                    *offset += new_text_size;
                }
            } else if edit.is_replacement() {
                let current_text_size = edit.range().len();
                let new_text_size = TextSize::of(edit.content().unwrap_or_default());
                match new_text_size.cmp(&current_text_size) {
                    Ordering::Less => {
                        for offset in &mut self.cell_offsets[idx + 1..] {
                            *offset -= current_text_size - new_text_size;
                        }
                    }
                    Ordering::Greater => {
                        for offset in &mut self.cell_offsets[idx + 1..] {
                            *offset += new_text_size - current_text_size;
                        }
                    }
                    Ordering::Equal => (),
                };
            } else {
                panic!("unexpected edit: {edit:?}");
            }
        }
    }

    fn update_cell_content(&mut self, transformed: &str) {
        for (cell, (start, end)) in self
            .raw
            .cells
            .iter_mut()
            .filter(|cell| cell.cell_type == CellType::Code)
            .zip(self.cell_offsets.iter().tuple_windows::<(_, _)>())
        {
            let cell_content = transformed
                .get(start.to_usize()..end.to_usize())
                .unwrap_or_else(|| panic!("cell content out of bounds: {cell:?}"))
                .trim_end_matches(|c| c == '\r' || c == '\n')
                .to_string();
            cell.source = SourceValue::String(cell_content);
        }
    }

    fn refresh_index(&mut self) {
        let size_hint = self
            .raw
            .cells
            .iter()
            .filter(|cell| cell.cell_type == CellType::Code)
            .count();

        let mut contents = Vec::with_capacity(size_hint);
        let mut builder = JupyterIndexBuilder::default();

        for (pos, cell) in self
            .raw
            .cells
            .iter()
            .enumerate()
            .filter(|(_pos, cell)| cell.cell_type == CellType::Code)
        {
            let cell_contents = builder.add_cell(pos, cell);
            contents.push(cell_contents);
        }

        self.index = builder.finish();
        self.content = contents.join("\n");
    }

    pub fn update(&mut self, edits: BTreeSet<&Edit>, transformed: &str) {
        self.update_cell_offsets(edits);
        self.update_cell_content(transformed);
        self.refresh_index();
    }

    /// Return `true` if the notebook is a Python notebook, `false` otherwise.
    pub fn is_python_notebook(&self) -> bool {
        self.raw
            .metadata
            .language_info
            .as_ref()
            .map_or(true, |language| language.name == "python")
    }

    /// Write back with an indent of 1, just like black
    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        let mut writer = BufWriter::new(File::create(path)?);
        // https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#LL1041
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut writer, formatter);
        self.raw.serialize(&mut ser)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::sync::Arc;

    #[cfg(feature = "jupyter_notebook")]
    use crate::jupyter::index::{JupyterIndex, JupyterIndexInner};
    use crate::jupyter::is_jupyter_notebook;
    use crate::jupyter::Notebook;

    #[test]
    fn test_valid() {
        let path = Path::new("resources/test/fixtures/jupyter/valid.ipynb");
        assert!(Notebook::read(path).is_ok());
    }

    #[test]
    fn test_r() {
        // We can load this, it will be filtered out later
        let path = Path::new("resources/test/fixtures/jupyter/R.ipynb");
        assert!(Notebook::read(path).is_ok());
    }

    #[test]
    fn test_invalid() {
        let path = Path::new("resources/test/fixtures/jupyter/invalid_extension.ipynb");
        assert_eq!(
            Notebook::read(path).unwrap_err().kind.body,
            "SyntaxError: Expected a Jupyter Notebook (.ipynb extension), \
            which must be internally stored as JSON, \
            but found a Python source file: \
            expected value at line 1 column 1"
        );
        let path = Path::new("resources/test/fixtures/jupyter/not_json.ipynb");
        assert_eq!(
            Notebook::read(path).unwrap_err().kind.body,
            "SyntaxError: A Jupyter Notebook (.ipynb) must internally be JSON, \
            but this file isn't valid JSON: \
            expected value at line 1 column 1"
        );
        let path = Path::new("resources/test/fixtures/jupyter/wrong_schema.ipynb");
        assert_eq!(
            Notebook::read(path).unwrap_err().kind.body,
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
        let notebook = Notebook::read(path).unwrap();
        assert_eq!(
            notebook.content,
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
            notebook.index,
            JupyterIndex {
                inner: Arc::new(JupyterIndexInner {
                    row_to_cell: vec![0, 1, 1, 1, 1, 1, 1, 3, 3, 3, 3],
                    row_to_row_in_cell: vec![0, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4],
                }),
            }
        );
    }
}
