use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::iter;
use std::path::Path;

use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Serialize;
use serde_json::error::Category;

use ruff_diagnostics::Diagnostic;
use ruff_newlines::NewlineWithTrailingNewline;
use ruff_text_size::{TextRange, TextSize};

use crate::autofix::source_map::{SourceMap, SourceMarker};
use crate::jupyter::index::JupyterIndex;
use crate::jupyter::{Cell, CellType, RawNotebook, SourceValue};
use crate::rules::pycodestyle::rules::SyntaxError;
use crate::IOError;

pub const JUPYTER_NOTEBOOK_EXT: &str = "ipynb";

const MAGIC_PREFIX: [&str; 3] = ["%", "!", "?"];

/// Return `true` if the [`Path`] appears to be that of a jupyter notebook file (`.ipynb`).
pub fn is_jupyter_notebook(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == JUPYTER_NOTEBOOK_EXT)
        // For now this is feature gated here, the long term solution depends on
        // https://github.com/astral-sh/ruff/issues/3410
        && cfg!(feature = "jupyter_notebook")
}

impl Cell {
    /// Return `true` if it's a valid code cell.
    ///
    /// A valid code cell is a cell where the type is [`CellType::Code`] and the
    /// source doesn't contain a magic, shell or help command.
    fn is_valid_code_cell(&self) -> bool {
        if self.cell_type != CellType::Code {
            return false;
        }
        // Ignore a cell if it contains a magic command. There could be valid
        // Python code as well, but we'll ignore that for now.
        // TODO(dhruvmanila): https://github.com/psf/black/blob/main/src/black/handle_ipynb_magics.py
        !match &self.source {
            SourceValue::String(string) => string.lines().any(|line| {
                MAGIC_PREFIX
                    .iter()
                    .any(|prefix| line.trim_start().starts_with(prefix))
            }),
            SourceValue::StringArray(string_array) => string_array.iter().any(|line| {
                MAGIC_PREFIX
                    .iter()
                    .any(|prefix| line.trim_start().starts_with(prefix))
            }),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Notebook {
    /// Python source code of the notebook.
    ///
    /// This is the concatenation of all valid code cells in the notebook
    /// separated by a newline and a trailing newline. The trailing newline
    /// is added to make sure that each cell ends with a newline which will
    /// be removed when updating the cell content.
    content: String,
    /// The index of the notebook. This is used to map between the concatenated
    /// source code and the original notebook.
    index: OnceCell<JupyterIndex>,
    /// The raw notebook i.e., the deserialized version of JSON string.
    raw: RawNotebook,
    /// The offsets of each cell in the concatenated source code. This includes
    /// the first and last character offsets as well.
    cell_offsets: Vec<TextSize>,
    /// The cell numbers of all valid code cells in the notebook.
    valid_code_cells: Vec<u32>,
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
        let notebook: RawNotebook = match serde_json::from_reader(reader) {
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

        let valid_code_cells = notebook
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.is_valid_code_cell())
            .map(|(pos, _)| u32::try_from(pos).unwrap())
            .collect::<Vec<_>>();

        let mut contents = Vec::with_capacity(valid_code_cells.len());
        let mut current_offset = TextSize::from(0);
        let mut cell_offsets = Vec::with_capacity(notebook.cells.len());
        cell_offsets.push(TextSize::from(0));

        for &pos in &valid_code_cells {
            let cell_contents = match &notebook.cells[pos as usize].source {
                SourceValue::String(string) => string.clone(),
                SourceValue::StringArray(string_array) => string_array.join(""),
            };
            current_offset += TextSize::of(&cell_contents) + TextSize::new(1);
            cell_offsets.push(current_offset);
            contents.push(cell_contents);
        }

        Ok(Self {
            raw: notebook,
            index: OnceCell::new(),
            // The additional newline at the end is to maintain consistency for
            // all cells. These newlines will be removed before updating the
            // source code with the transformed content. Refer `update_cell_content`.
            content: contents.join("\n") + "\n",
            cell_offsets,
            valid_code_cells,
        })
    }

    fn update_cell_offsets(&mut self, source_map: &SourceMap) {
        let mut last_marker: Option<&SourceMarker> = None;

        // The first offset is always going to be at 0, so skip it.
        for offset in self.cell_offsets.iter_mut().skip(1).rev() {
            let closest_marker = match last_marker {
                Some(marker) if marker.source <= *offset => marker,
                _ => {
                    let Some(marker) = source_map
                        .markers()
                        .iter()
                        .rev()
                        .find(|m| m.source <= *offset) else {
                            // There are no markers above the current offset, so we can
                            // stop here.
                            break;
                        };
                    last_marker = Some(marker);
                    marker
                }
            };

            match closest_marker.source.cmp(&closest_marker.dest) {
                Ordering::Less => *offset += closest_marker.dest - closest_marker.source,
                Ordering::Greater => *offset -= closest_marker.source - closest_marker.dest,
                Ordering::Equal => (),
            }
        }
    }

    fn update_cell_content(&mut self, transformed: &str) {
        for (&pos, (start, end)) in self
            .valid_code_cells
            .iter()
            .zip(self.cell_offsets.iter().tuple_windows::<(_, _)>())
        {
            let cell_content = transformed
                .get(start.to_usize()..end.to_usize())
                .unwrap_or_else(|| {
                    panic!("cell content out of bounds ({start:?}..{end:?}): {transformed}")
                });
            self.raw.cells[pos as usize].source = SourceValue::String(
                cell_content
                    .strip_suffix('\n')
                    .unwrap_or(cell_content)
                    .to_string(),
            );
        }
    }

    /// Build and return the [`JupyterIndex`].
    fn build_index(&self) -> JupyterIndex {
        let mut row_to_cell = vec![0];
        let mut row_to_row_in_cell = vec![0];

        for &pos in &self.valid_code_cells {
            match &self.raw.cells[pos as usize].source {
                SourceValue::String(string) => {
                    let line_count =
                        u32::try_from(NewlineWithTrailingNewline::from(string).count()).unwrap();
                    row_to_cell.extend(iter::repeat(pos + 1).take(line_count as usize));
                    row_to_row_in_cell.extend(1..=line_count);
                }
                SourceValue::StringArray(string_array) => {
                    // Trailing newlines for each line are part of the string itself.
                    // So, to count the actual number of visible lines, we need to
                    // check for any trailing newline for the last line.
                    //
                    // ```python
                    // [
                    //     "import os\n",
                    //     "import sys\n",
                    // ]
                    // ```
                    //
                    // Here, the array suggests 2 lines but there are 3 visible lines.
                    let trailing_newline =
                        usize::from(string_array.last().map_or(false, |s| s.ends_with('\n')));
                    row_to_cell
                        .extend(iter::repeat(pos + 1).take(string_array.len() + trailing_newline));
                    row_to_row_in_cell
                        .extend(1..=u32::try_from(string_array.len() + trailing_newline).unwrap());
                }
            }
        }

        JupyterIndex {
            row_to_cell,
            row_to_row_in_cell,
        }
    }

    /// Return the notebook content.
    ///
    /// This is the concatenation of all Python code cells.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Return the Jupyter notebook index.
    ///
    /// The index is built only once when required. This is only used to
    /// report diagnostics, so by that time all of the autofixes must have
    /// been applied if `--fix` was passed.
    pub fn index(&self) -> &JupyterIndex {
        self.index.get_or_init(|| self.build_index())
    }

    /// Return the cell offsets for the concatenated source code corresponding
    /// the Jupyter notebook.
    pub fn cell_offsets(&self) -> &[TextSize] {
        &self.cell_offsets
    }

    /// Update the notebook with the given edits and transformed content.
    pub fn update(&mut self, source_map: &SourceMap, transformed: &str) {
        self.update_cell_offsets(source_map);
        self.update_cell_content(transformed);
        self.content = transformed.to_string();
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

    use anyhow::Result;
    use test_case::test_case;

    use crate::jupyter::index::JupyterIndex;
    #[cfg(feature = "jupyter_notebook")]
    use crate::jupyter::is_jupyter_notebook;
    use crate::jupyter::schema::Cell;
    use crate::jupyter::Notebook;

    use crate::test::test_resource_path;

    /// Read a Jupyter cell from the `resources/test/fixtures/jupyter/cell` directory.
    fn read_jupyter_cell(path: impl AsRef<Path>) -> Result<Cell> {
        let path = test_resource_path("fixtures/jupyter/cell").join(path);
        let contents = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&contents)?)
    }

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

    #[test_case(Path::new("markdown.json"), false; "markdown")]
    #[test_case(Path::new("python_magic.json"), false; "python_magic")]
    #[test_case(Path::new("python_no_magic.json"), true; "python_no_magic")]
    fn test_is_valid_code_cell(path: &Path, expected: bool) -> Result<()> {
        assert_eq!(read_jupyter_cell(path)?.is_valid_code_cell(), expected);
        Ok(())
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
            notebook.index(),
            &JupyterIndex {
                row_to_cell: vec![0, 1, 1, 1, 1, 1, 1, 3, 3, 3, 3, 3],
                row_to_row_in_cell: vec![0, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5],
            }
        );
    }
}
