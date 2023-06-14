use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Write};
use std::iter;
use std::path::Path;

use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Serialize;
use serde_json::error::Category;

use ruff_diagnostics::Diagnostic;
use ruff_python_whitespace::NewlineWithTrailingNewline;
use ruff_text_size::{TextRange, TextSize};

use crate::autofix::source_map::{SourceMap, SourceMarker};
use crate::jupyter::index::JupyterIndex;
use crate::jupyter::{Cell, CellType, RawNotebook, SourceValue};
use crate::rules::pycodestyle::rules::SyntaxError;
use crate::IOError;

pub const JUPYTER_NOTEBOOK_EXT: &str = "ipynb";

const MAGIC_PREFIX: [&str; 3] = ["%", "!", "?"];

/// Run round-trip source code generation on a given Jupyter notebook file path.
pub fn round_trip(path: &Path) -> anyhow::Result<String> {
    let mut notebook = Notebook::read(path).map_err(|err| {
        anyhow::anyhow!(
            "Failed to read notebook file `{}`: {:?}",
            path.display(),
            err
        )
    })?;
    let code = notebook.content().to_string();
    notebook.update_cell_content(&code);
    let mut buffer = Cursor::new(Vec::new());
    notebook.write_inner(&mut buffer)?;
    Ok(String::from_utf8(buffer.into_inner())?)
}

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
            contents.push(cell_contents);
            cell_offsets.push(current_offset);
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

    /// Update the cell offsets as per the given [`SourceMap`].
    fn update_cell_offsets(&mut self, source_map: &SourceMap) {
        // When there are multiple cells without any edits, the offsets of those
        // cells will be updated using the same marker. So, we can keep track of
        // the last marker used to update the offsets and check if it's still
        // the closest marker to the current offset.
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

    /// Update the cell contents with the transformed content.
    ///
    /// ## Panics
    ///
    /// Panics if the transformed content is out of bounds for any cell. This
    /// can happen only if the cell offsets were not updated before calling
    /// this method or the offsets were updated incorrectly.
    fn update_cell_content(&mut self, transformed: &str) {
        for (&pos, (start, end)) in self
            .valid_code_cells
            .iter()
            .zip(self.cell_offsets.iter().tuple_windows::<(_, _)>())
        {
            let cell_content = transformed
                .get(start.to_usize()..end.to_usize())
                .unwrap_or_else(|| {
                    panic!("Transformed content out of bounds ({start:?}..{end:?}) for cell {pos}");
                });
            self.raw.cells[pos as usize].source = SourceValue::String(
                cell_content
                    // We only need to strip the trailing newline which we added
                    // while concatenating the cell contents.
                    .strip_suffix('\n')
                    .unwrap_or(cell_content)
                    .to_string(),
            );
        }
    }

    /// Build and return the [`JupyterIndex`].
    ///
    /// # Notes
    ///
    /// Empty cells don't have any newlines, but there's a single visible line
    /// in the UI. That single line needs to be accounted for.
    ///
    /// In case of [`SourceValue::StringArray`], newlines are part of the strings.
    /// So, to get the actual count of lines, we need to check for any trailing
    /// newline for the last line.
    ///
    /// For example, consider the following cell:
    /// ```python
    /// [
    ///    "import os\n",
    ///    "import sys\n",
    /// ]
    /// ```
    ///
    /// Here, the array suggests that there are two lines, but the actual number
    /// of lines visible in the UI is three. The same goes for [`SourceValue::String`]
    /// where we need to check for the trailing newline.
    ///
    /// The index building is expensive as it needs to go through the content of
    /// every valid code cell.
    fn build_index(&self) -> JupyterIndex {
        let mut row_to_cell = vec![0];
        let mut row_to_row_in_cell = vec![0];

        for &pos in &self.valid_code_cells {
            let line_count = match &self.raw.cells[pos as usize].source {
                SourceValue::String(string) => {
                    if string.is_empty() {
                        1
                    } else {
                        u32::try_from(NewlineWithTrailingNewline::from(string).count()).unwrap()
                    }
                }
                SourceValue::StringArray(string_array) => {
                    if string_array.is_empty() {
                        1
                    } else {
                        let trailing_newline =
                            usize::from(string_array.last().map_or(false, |s| s.ends_with('\n')));
                        u32::try_from(string_array.len() + trailing_newline).unwrap()
                    }
                }
            };
            row_to_cell.extend(iter::repeat(pos + 1).take(line_count as usize));
            row_to_row_in_cell.extend(1..=line_count);
        }

        JupyterIndex {
            row_to_cell,
            row_to_row_in_cell,
        }
    }

    /// Return the notebook content.
    ///
    /// This is the concatenation of all Python code cells.
    pub(crate) fn content(&self) -> &str {
        &self.content
    }

    /// Return the Jupyter notebook index.
    ///
    /// The index is built only once when required. This is only used to
    /// report diagnostics, so by that time all of the autofixes must have
    /// been applied if `--fix` was passed.
    pub(crate) fn index(&self) -> &JupyterIndex {
        self.index.get_or_init(|| self.build_index())
    }

    /// Return the cell offsets for the concatenated source code corresponding
    /// the Jupyter notebook.
    pub(crate) fn cell_offsets(&self) -> &[TextSize] {
        &self.cell_offsets
    }

    /// Update the notebook with the given sourcemap and transformed content.
    pub(crate) fn update(&mut self, source_map: &SourceMap, transformed: &str) {
        // Cell offsets must be updated before updating the cell content as
        // it depends on the offsets to extract the cell content.
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

    fn write_inner(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        // https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#LL1041
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(writer, formatter);
        self.raw.serialize(&mut ser)?;
        Ok(())
    }

    /// Write back with an indent of 1, just like black
    pub fn write(&self, path: &Path) -> anyhow::Result<()> {
        let mut writer = BufWriter::new(File::create(path)?);
        self.write_inner(&mut writer)?;
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
    use crate::registry::Rule;
    use crate::test::{test_notebook_path, test_resource_path};
    use crate::{assert_messages, settings};

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
    #[test_case(Path::new("only_magic.json"), false; "only_magic")]
    #[test_case(Path::new("code_and_magic.json"), false; "code_and_magic")]
    #[test_case(Path::new("only_code.json"), true; "only_code")]
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




print("after empty cells")
"#
        );
        assert_eq!(
            notebook.index(),
            &JupyterIndex {
                row_to_cell: vec![0, 1, 1, 1, 1, 1, 1, 3, 3, 3, 3, 3, 5, 7, 7, 8],
                row_to_row_in_cell: vec![0, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 1, 1, 2, 1],
            }
        );
        assert_eq!(
            notebook.cell_offsets(),
            &[
                0.into(),
                90.into(),
                168.into(),
                169.into(),
                171.into(),
                198.into()
            ]
        );
    }

    #[test]
    fn test_import_sorting() -> Result<()> {
        let diagnostics = test_notebook_path(
            Path::new("isort.ipynb"),
            Path::new("isort_expected.ipynb"),
            &settings::Settings::for_rule(Rule::UnsortedImports),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
