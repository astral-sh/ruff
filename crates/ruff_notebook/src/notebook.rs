use std::cmp::Ordering;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::{io, iter};

use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Serialize;
use serde_json::error::Category;
use thiserror::Error;
use uuid::Uuid;

use ruff_diagnostics::{SourceMap, SourceMarker};
use ruff_source_file::{NewlineWithTrailingNewline, UniversalNewlineIterator};
use ruff_text_size::TextSize;

use crate::index::NotebookIndex;
use crate::schema::{Cell, RawNotebook, SortAlphabetically, SourceValue};

/// Run round-trip source code generation on a given Jupyter notebook file path.
pub fn round_trip(path: &Path) -> anyhow::Result<String> {
    let mut notebook = Notebook::from_path(path).map_err(|err| {
        anyhow::anyhow!(
            "Failed to read notebook file `{}`: {:?}",
            path.display(),
            err
        )
    })?;
    let code = notebook.source_code().to_string();
    notebook.update_cell_content(&code);
    let mut writer = Vec::new();
    notebook.write(&mut writer)?;
    Ok(String::from_utf8(writer)?)
}

impl Display for SourceValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceValue::String(string) => f.write_str(string),
            SourceValue::StringArray(string_array) => {
                for string in string_array {
                    f.write_str(string)?;
                }
                Ok(())
            }
        }
    }
}

impl Cell {
    /// Return the [`SourceValue`] of the cell.
    fn source(&self) -> &SourceValue {
        match self {
            Cell::Code(cell) => &cell.source,
            Cell::Markdown(cell) => &cell.source,
            Cell::Raw(cell) => &cell.source,
        }
    }

    /// Update the [`SourceValue`] of the cell.
    fn set_source(&mut self, source: SourceValue) {
        match self {
            Cell::Code(cell) => cell.source = source,
            Cell::Markdown(cell) => cell.source = source,
            Cell::Raw(cell) => cell.source = source,
        }
    }

    /// Return `true` if it's a valid code cell.
    ///
    /// A valid code cell is a cell where the cell type is [`Cell::Code`] and the
    /// source doesn't contain a cell magic.
    fn is_valid_code_cell(&self) -> bool {
        let source = match self {
            Cell::Code(cell) => &cell.source,
            _ => return false,
        };
        // Ignore cells containing cell magic as they act on the entire cell
        // as compared to line magic which acts on a single line.
        !match source {
            SourceValue::String(string) => string
                .lines()
                .any(|line| line.trim_start().starts_with("%%")),
            SourceValue::StringArray(string_array) => string_array
                .iter()
                .any(|line| line.trim_start().starts_with("%%")),
        }
    }
}

/// An error that can occur while deserializing a Jupyter Notebook.
#[derive(Error, Debug)]
pub enum NotebookError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(serde_json::Error),
    #[error("Expected a Jupyter Notebook, which must be internally stored as JSON, but this file isn't valid JSON: {0}")]
    InvalidJson(serde_json::Error),
    #[error("This file does not match the schema expected of Jupyter Notebooks: {0}")]
    InvalidSchema(serde_json::Error),
    #[error("Expected Jupyter Notebook format 4, found: {0}")]
    InvalidFormat(i64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notebook {
    /// Python source code of the notebook.
    ///
    /// This is the concatenation of all valid code cells in the notebook
    /// separated by a newline and a trailing newline. The trailing newline
    /// is added to make sure that each cell ends with a newline which will
    /// be removed when updating the cell content.
    source_code: String,
    /// The index of the notebook. This is used to map between the concatenated
    /// source code and the original notebook.
    index: OnceCell<NotebookIndex>,
    /// The raw notebook i.e., the deserialized version of JSON string.
    raw: RawNotebook,
    /// The offsets of each cell in the concatenated source code. This includes
    /// the first and last character offsets as well.
    cell_offsets: Vec<TextSize>,
    /// The cell index of all valid code cells in the notebook.
    valid_code_cells: Vec<u32>,
    /// Flag to indicate if the JSON string of the notebook has a trailing newline.
    trailing_newline: bool,
}

impl Notebook {
    /// Read the Jupyter Notebook from the given [`Path`].
    pub fn from_path(path: &Path) -> Result<Self, NotebookError> {
        Self::from_reader(BufReader::new(File::open(path)?))
    }

    /// Read the Jupyter Notebook from its JSON string.
    pub fn from_source_code(source_code: &str) -> Result<Self, NotebookError> {
        Self::from_reader(Cursor::new(source_code))
    }

    /// Read a Jupyter Notebook from a [`Read`] implementor.
    ///
    /// See also the black implementation
    /// <https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#L1017-L1046>
    fn from_reader<R>(mut reader: R) -> Result<Self, NotebookError>
    where
        R: Read + Seek,
    {
        let trailing_newline = reader.seek(SeekFrom::End(-1)).is_ok_and(|_| {
            let mut buf = [0; 1];
            reader.read_exact(&mut buf).is_ok_and(|_| buf[0] == b'\n')
        });
        reader.rewind()?;
        let mut raw_notebook: RawNotebook = match serde_json::from_reader(reader.by_ref()) {
            Ok(notebook) => notebook,
            Err(err) => {
                // Translate the error into a diagnostic
                return Err(match err.classify() {
                    Category::Io => NotebookError::Json(err),
                    Category::Syntax | Category::Eof => NotebookError::InvalidJson(err),
                    Category::Data => {
                        // We could try to read the schema version here but if this fails it's
                        // a bug anyway.
                        NotebookError::InvalidSchema(err)
                    }
                });
            }
        };

        // v4 is what everybody uses
        if raw_notebook.nbformat != 4 {
            // bail because we should have already failed at the json schema stage
            return Err(NotebookError::InvalidFormat(raw_notebook.nbformat));
        }

        let valid_code_cells = raw_notebook
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.is_valid_code_cell())
            .map(|(idx, _)| u32::try_from(idx).unwrap())
            .collect::<Vec<_>>();

        let mut contents = Vec::with_capacity(valid_code_cells.len());
        let mut current_offset = TextSize::from(0);
        let mut cell_offsets = Vec::with_capacity(valid_code_cells.len());
        cell_offsets.push(TextSize::from(0));

        for &idx in &valid_code_cells {
            let cell_contents = match &raw_notebook.cells[idx as usize].source() {
                SourceValue::String(string) => string.clone(),
                SourceValue::StringArray(string_array) => string_array.join(""),
            };
            current_offset += TextSize::of(&cell_contents) + TextSize::new(1);
            contents.push(cell_contents);
            cell_offsets.push(current_offset);
        }

        // Add cell ids to 4.5+ notebooks if they are missing
        // https://github.com/astral-sh/ruff/issues/6834
        // https://github.com/jupyter/enhancement-proposals/blob/master/62-cell-id/cell-id.md#required-field
        if raw_notebook.nbformat == 4 && raw_notebook.nbformat_minor >= 5 {
            for cell in &mut raw_notebook.cells {
                let id = match cell {
                    Cell::Code(cell) => &mut cell.id,
                    Cell::Markdown(cell) => &mut cell.id,
                    Cell::Raw(cell) => &mut cell.id,
                };
                if id.is_none() {
                    // https://github.com/jupyter/enhancement-proposals/blob/master/62-cell-id/cell-id.md#questions
                    *id = Some(Uuid::new_v4().to_string());
                }
            }
        }

        Ok(Self {
            raw: raw_notebook,
            index: OnceCell::new(),
            // The additional newline at the end is to maintain consistency for
            // all cells. These newlines will be removed before updating the
            // source code with the transformed content. Refer `update_cell_content`.
            source_code: contents.join("\n") + "\n",
            cell_offsets,
            valid_code_cells,
            trailing_newline,
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
                Some(marker) if marker.source() <= *offset => marker,
                _ => {
                    let Some(marker) = source_map
                        .markers()
                        .iter()
                        .rev()
                        .find(|marker| marker.source() <= *offset)
                    else {
                        // There are no markers above the current offset, so we can
                        // stop here.
                        break;
                    };
                    last_marker = Some(marker);
                    marker
                }
            };

            match closest_marker.source().cmp(&closest_marker.dest()) {
                Ordering::Less => *offset += closest_marker.dest() - closest_marker.source(),
                Ordering::Greater => *offset -= closest_marker.source() - closest_marker.dest(),
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
        for (&idx, (start, end)) in self
            .valid_code_cells
            .iter()
            .zip(self.cell_offsets.iter().tuple_windows::<(_, _)>())
        {
            let cell_content = transformed
                .get(start.to_usize()..end.to_usize())
                .unwrap_or_else(|| {
                    panic!(
                        "Transformed content out of bounds ({start:?}..{end:?}) for cell at {idx:?}"
                    );
                });
            self.raw.cells[idx as usize].set_source(SourceValue::StringArray(
                UniversalNewlineIterator::from(
                    // We only need to strip the trailing newline which we added
                    // while concatenating the cell contents.
                    cell_content.strip_suffix('\n').unwrap_or(cell_content),
                )
                .map(|line| line.as_full_str().to_string())
                .collect::<Vec<_>>(),
            ));
        }
    }

    /// Build and return the [`JupyterIndex`].
    ///
    /// ## Notes
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
    fn build_index(&self) -> NotebookIndex {
        let mut row_to_cell = vec![0];
        let mut row_to_row_in_cell = vec![0];

        for &idx in &self.valid_code_cells {
            let line_count = match &self.raw.cells[idx as usize].source() {
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
                            usize::from(string_array.last().is_some_and(|s| s.ends_with('\n')));
                        u32::try_from(string_array.len() + trailing_newline).unwrap()
                    }
                }
            };
            row_to_cell.extend(iter::repeat(idx + 1).take(line_count as usize));
            row_to_row_in_cell.extend(1..=line_count);
        }

        NotebookIndex {
            row_to_cell,
            row_to_row_in_cell,
        }
    }

    /// Return the notebook content.
    ///
    /// This is the concatenation of all Python code cells.
    pub fn source_code(&self) -> &str {
        &self.source_code
    }

    /// Return the Jupyter notebook index.
    ///
    /// The index is built only once when required. This is only used to
    /// report diagnostics, so by that time all of the autofixes must have
    /// been applied if `--fix` was passed.
    pub fn index(&self) -> &NotebookIndex {
        self.index.get_or_init(|| self.build_index())
    }

    /// Return the cell offsets for the concatenated source code corresponding
    /// the Jupyter notebook.
    pub fn cell_offsets(&self) -> &[TextSize] {
        &self.cell_offsets
    }

    /// Return `true` if the notebook has a trailing newline, `false` otherwise.
    pub fn trailing_newline(&self) -> bool {
        self.trailing_newline
    }

    /// Update the notebook with the given sourcemap and transformed content.
    pub fn update(&mut self, source_map: &SourceMap, transformed: String) {
        // Cell offsets must be updated before updating the cell content as
        // it depends on the offsets to extract the cell content.
        self.index.take();
        self.update_cell_offsets(source_map);
        self.update_cell_content(&transformed);
        self.source_code = transformed;
    }

    /// Return a slice of [`Cell`] in the Jupyter notebook.
    pub fn cells(&self) -> &[Cell] {
        &self.raw.cells
    }

    /// Return `true` if the notebook is a Python notebook, `false` otherwise.
    pub fn is_python_notebook(&self) -> bool {
        self.raw
            .metadata
            .language_info
            .as_ref()
            .map_or(true, |language| language.name == "python")
    }

    /// Write the notebook back to the given [`Write`] implementor.
    pub fn write(&self, writer: &mut dyn Write) -> anyhow::Result<()> {
        // https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#LL1041
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut serializer = serde_json::Serializer::with_formatter(writer, formatter);
        SortAlphabetically(&self.raw).serialize(&mut serializer)?;
        if self.trailing_newline {
            writeln!(serializer.into_inner())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::{Cell, Notebook, NotebookError, NotebookIndex};

    /// Construct a path to a Jupyter notebook in the `resources/test/fixtures/jupyter` directory.
    fn notebook_path(path: impl AsRef<Path>) -> std::path::PathBuf {
        Path::new("./resources/test/fixtures/jupyter").join(path)
    }

    #[test]
    fn test_python() -> Result<(), NotebookError> {
        let notebook = Notebook::from_path(&notebook_path("valid.ipynb"))?;
        assert!(notebook.is_python_notebook());
        Ok(())
    }

    #[test]
    fn test_r() -> Result<(), NotebookError> {
        let notebook = Notebook::from_path(&notebook_path("R.ipynb"))?;
        assert!(!notebook.is_python_notebook());
        Ok(())
    }

    #[test]
    fn test_invalid() {
        assert!(matches!(
            Notebook::from_path(&notebook_path("invalid_extension.ipynb")),
            Err(NotebookError::InvalidJson(_))
        ));
        assert!(matches!(
            Notebook::from_path(&notebook_path("not_json.ipynb")),
            Err(NotebookError::InvalidJson(_))
        ));
        assert!(matches!(
            Notebook::from_path(&notebook_path("wrong_schema.ipynb")),
            Err(NotebookError::InvalidSchema(_))
        ));
    }

    #[test_case(Path::new("markdown.json"), false; "markdown")]
    #[test_case(Path::new("only_magic.json"), true; "only_magic")]
    #[test_case(Path::new("code_and_magic.json"), true; "code_and_magic")]
    #[test_case(Path::new("only_code.json"), true; "only_code")]
    #[test_case(Path::new("cell_magic.json"), false; "cell_magic")]
    fn test_is_valid_code_cell(path: &Path, expected: bool) -> Result<()> {
        /// Read a Jupyter cell from the `resources/test/fixtures/jupyter/cell` directory.
        fn read_jupyter_cell(path: impl AsRef<Path>) -> Result<Cell> {
            let path = notebook_path("cell").join(path);
            let source_code = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&source_code)?)
        }

        assert_eq!(read_jupyter_cell(path)?.is_valid_code_cell(), expected);
        Ok(())
    }

    #[test]
    fn test_concat_notebook() -> Result<(), NotebookError> {
        let notebook = Notebook::from_path(&notebook_path("valid.ipynb"))?;
        assert_eq!(
            notebook.source_code,
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
            &NotebookIndex {
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
        Ok(())
    }
}
