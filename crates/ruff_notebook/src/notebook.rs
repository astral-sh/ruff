use itertools::Itertools;
use rand::{RngExt, SeedableRng};
use serde::Serialize;
use serde_json::error::Category;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::OnceLock;
use thiserror::Error;

use ruff_diagnostics::{SourceMap, SourceMarker};
use ruff_source_file::{OneIndexed, UniversalNewlineIterator};
use ruff_text_size::{TextRange, TextSize};

use crate::cell::CellOffsets;
use crate::index::NotebookIndex;
use crate::schema::{Cell, RawNotebook, SortAlphabetically, SourceValue};
use crate::{CellMetadata, CellStart, RawNotebookMetadata, SYNTHETIC_CELL_SEPARATOR, schema};

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
    let needs_rebuild = notebook.update_cell_content(&code);
    debug_assert!(
        !needs_rebuild,
        "round-tripping unchanged source cannot remove a synthetic cell separator"
    );
    let mut writer = Vec::new();
    notebook.write(&mut writer)?;
    Ok(String::from_utf8(writer)?)
}

/// An error that can occur while deserializing a Jupyter Notebook.
#[derive(Error, Debug)]
pub enum NotebookError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(serde_json::Error),
    #[error(
        "Expected a Jupyter Notebook, which must be internally stored as JSON, but this file isn't valid JSON: {0}"
    )]
    InvalidJson(serde_json::Error),
    #[error("This file does not match the schema expected of Jupyter Notebooks: {0}")]
    InvalidSchema(serde_json::Error),
    #[error("Expected Jupyter Notebook format 4, found: {0}")]
    InvalidFormat(i64),
}

#[derive(Clone, Debug)]
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
    index: OnceLock<NotebookIndex>,
    /// The raw notebook i.e., the deserialized version of JSON string.
    raw: RawNotebook,
    /// The offsets of each cell in the concatenated source code. This includes
    /// the first and last character offsets as well.
    cell_offsets: CellOffsets,
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

    /// Read a Jupyter Notebook from a [`Read`] implementer.
    ///
    /// See also the black implementation
    /// <https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#L1017-L1046>
    fn from_reader<R>(mut reader: R) -> Result<Self, NotebookError>
    where
        R: Read + Seek,
    {
        let trailing_newline = reader.seek(SeekFrom::End(-1)).is_ok_and(|_| {
            let mut buf = [0; 1];
            reader.read_exact(&mut buf).is_ok_and(|()| buf[0] == b'\n')
        });
        reader.rewind()?;
        let raw_notebook: RawNotebook = match serde_json::from_reader(reader.by_ref()) {
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
        Self::from_raw_notebook(raw_notebook, trailing_newline)
    }

    pub fn from_raw_notebook(
        mut raw_notebook: RawNotebook,
        trailing_newline: bool,
    ) -> Result<Self, NotebookError> {
        // v4 is what everybody uses
        if raw_notebook.nbformat != 4 {
            // bail because we should have already failed at the json schema stage
            return Err(NotebookError::InvalidFormat(raw_notebook.nbformat));
        }

        let valid_code_cells = raw_notebook
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.is_valid_python_code_cell())
            .map(|(cell_index, _)| u32::try_from(cell_index).unwrap())
            .collect::<Vec<_>>();

        // Add cell ids to 4.5+ notebooks if they are missing
        // https://github.com/astral-sh/ruff/issues/6834
        // https://github.com/jupyter/enhancement-proposals/blob/master/62-cell-id/cell-id.md#required-field
        // https://github.com/jupyter/enhancement-proposals/blob/master/62-cell-id/cell-id.md#questions
        if raw_notebook.nbformat == 4 && raw_notebook.nbformat_minor >= 5 {
            // We use a insecure random number generator to generate deterministic uuids
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            let mut existing_ids = HashSet::new();

            for cell in &raw_notebook.cells {
                let id = match cell {
                    Cell::Code(cell) => &cell.id,
                    Cell::Markdown(cell) => &cell.id,
                    Cell::Raw(cell) => &cell.id,
                };
                if let Some(id) = id {
                    existing_ids.insert(id.clone());
                }
            }

            for cell in &mut raw_notebook.cells {
                let id = match cell {
                    Cell::Code(cell) => &mut cell.id,
                    Cell::Markdown(cell) => &mut cell.id,
                    Cell::Raw(cell) => &mut cell.id,
                };
                if id.is_none() {
                    loop {
                        let new_id = uuid::Builder::from_random_bytes(rng.random())
                            .into_uuid()
                            .as_simple()
                            .to_string();

                        if existing_ids.insert(new_id.clone()) {
                            *id = Some(new_id);
                            break;
                        }
                    }
                }
            }
        }

        let (source_code, cell_offsets) =
            Self::source_code_and_cell_offsets(&raw_notebook, &valid_code_cells);

        Ok(Self {
            raw: raw_notebook,
            index: OnceLock::new(),
            source_code,
            cell_offsets,
            valid_code_cells,
            trailing_newline,
        })
    }

    /// Creates an empty notebook with a single code cell.
    pub fn empty() -> Self {
        Self::from_raw_notebook(
            RawNotebook {
                cells: vec![schema::Cell::Code(schema::CodeCell {
                    execution_count: None,
                    id: None,
                    metadata: CellMetadata::default(),
                    outputs: vec![],
                    source: schema::SourceValue::String(String::default()),
                })],
                metadata: RawNotebookMetadata::default(),
                nbformat: 4,
                nbformat_minor: 5,
            },
            false,
        )
        .unwrap()
    }

    /// Build the concatenated source code and cell offsets from the raw notebook.
    fn source_code_and_cell_offsets(
        raw_notebook: &RawNotebook,
        valid_code_cells: &[u32],
    ) -> (String, CellOffsets) {
        let mut source_code = String::new();
        let mut cell_offsets = CellOffsets::with_capacity(valid_code_cells.len() + 1);
        cell_offsets.push(TextSize::from(0));

        for &idx in valid_code_cells {
            match raw_notebook.cells[idx as usize].source() {
                SourceValue::String(string) => source_code.push_str(string),
                SourceValue::StringArray(string_array) => {
                    for string in string_array {
                        source_code.push_str(string);
                    }
                }
            }
            source_code.push(SYNTHETIC_CELL_SEPARATOR);
            cell_offsets.push(TextSize::of(&source_code));
        }

        // The additional newline maintains a consistent source representation
        // for notebooks without any valid Python code cells. Synthetic newlines
        // are removed before updating the raw cell content. Refer
        // `update_cell_content`.
        if valid_code_cells.is_empty() {
            source_code.push(SYNTHETIC_CELL_SEPARATOR);
        }

        (source_code, cell_offsets)
    }

    /// Update the cell offsets as per the given [`SourceMap`].
    fn update_cell_offsets(&mut self, source_map: &SourceMap) {
        // When there are multiple cells without any edits, the offsets of those
        // cells will be updated using the same marker. So, we can keep track of
        // the last marker used to update the offsets and check if it's still
        // the closest marker to the current offset.
        let mut last_marker: Option<&SourceMarker> = None;

        // The first offset is always going to be at 0, so skip it.
        for (index, offset) in self.cell_offsets.iter_mut().skip(1).rev().enumerate() {
            let closest_marker = match last_marker {
                Some(marker) if marker.source() < *offset => marker,
                _ => {
                    let mut markers = source_map.markers().iter().rev();
                    let Some(marker) = markers.find(|marker| marker.source() <= *offset) else {
                        // There are no markers above the current offset, so we can
                        // stop here.
                        break;
                    };
                    // An internal offset is also the start of the following cell, so prefer the
                    // first marker at that offset. The final offset is only a cell end.
                    let marker = if index > 0 && marker.source() == *offset {
                        markers
                            .take_while(|marker| marker.source() == *offset)
                            .last()
                            .unwrap_or(marker)
                    } else {
                        marker
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
    /// Returns `true` if a cell separator was removed and the source code and
    /// cell offsets need to be rebuilt.
    ///
    /// ## Panics
    ///
    /// Panics if the transformed content is out of bounds for any cell. This
    /// can happen only if the cell offsets were not updated before calling
    /// this method or the offsets were updated incorrectly.
    fn update_cell_content(&mut self, transformed: &str) -> bool {
        let mut missing_separator = false;

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
            missing_separator |= !cell_content.ends_with(SYNTHETIC_CELL_SEPARATOR);
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

        missing_separator
    }

    /// Build and return the [`NotebookIndex`].
    ///
    /// ## Notes
    ///
    /// Each cell range includes its synthetic newline separator. Counting the
    /// lines in the concatenated source accounts for empty cells and for cells
    /// that already end in a newline.
    ///
    /// For example, the source array:
    /// ```text
    /// ["import os\n", "import sys\n"]
    /// ```
    /// is joined with the synthetic separator to form `"import os\nimport sys\n\n"`,
    /// which occupies three rows. Array entries aren't necessarily lines, though:
    /// `["p", "a", "s", "s"]` is joined with the separator to form `"pass\n"` and
    /// occupies one row.
    fn build_index(&self) -> NotebookIndex {
        let mut cell_starts = Vec::with_capacity(self.valid_code_cells.len());

        let mut current_row = OneIndexed::MIN;

        for (&cell_index, range) in self.valid_code_cells.iter().zip(self.cell_offsets.ranges()) {
            let raw_cell_index = cell_index as usize;
            // Record the starting row of this cell
            cell_starts.push(CellStart {
                start_row: current_row,
                raw_cell_index: OneIndexed::from_zero_indexed(raw_cell_index),
            });

            let line_count = UniversalNewlineIterator::from(&self.source_code[range]).count();

            current_row = current_row.saturating_add(line_count);
        }

        NotebookIndex { cell_starts }
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
    /// report diagnostics, so by that time all of the fixes must have
    /// been applied if `--fix` was passed.
    pub fn index(&self) -> &NotebookIndex {
        self.index.get_or_init(|| self.build_index())
    }

    /// Return the Jupyter notebook index, consuming the notebook.
    ///
    /// The index is built only once when required. This is only used to
    /// report diagnostics, so by that time all of the fixes must have
    /// been applied if `--fix` was passed.
    pub fn into_index(mut self) -> NotebookIndex {
        self.index.take().unwrap_or_else(|| self.build_index())
    }

    /// Return the [`CellOffsets`] for the concatenated source code corresponding
    /// the Jupyter notebook.
    pub fn cell_offsets(&self) -> &CellOffsets {
        &self.cell_offsets
    }

    /// Returns the start offset of the cell at index `cell` in the concatenated
    /// text document.
    pub fn cell_offset(&self, cell: OneIndexed) -> Option<TextSize> {
        self.cell_offsets.get(cell.to_zero_indexed()).copied()
    }

    /// Returns the text range in the concatenated document of the cell
    /// with index `cell`.
    pub fn cell_range(&self, cell: OneIndexed) -> Option<TextRange> {
        let start = self.cell_offsets.get(cell.to_zero_indexed()).copied()?;
        let end = self.cell_offsets.get(cell.to_zero_indexed() + 1).copied()?;

        Some(TextRange::new(start, end))
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

        let needs_rebuild = self.update_cell_content(&transformed);

        if needs_rebuild {
            // A fix that empties a cell can also remove its synthetic newline
            // separator. For example, deleting `"import os\n"` from the first
            // cell changes offsets `[0, 10, 16]` to `[0, 0, 6]`. Rebuild the
            // source and offsets to restore the separator as `[0, 1, 7]`.
            (self.source_code, self.cell_offsets) =
                Self::source_code_and_cell_offsets(&self.raw, &self.valid_code_cells);
        } else {
            self.source_code = transformed;
        }
    }

    /// Return a slice of [`Cell`] in the Jupyter notebook.
    pub fn cells(&self) -> &[Cell] {
        &self.raw.cells
    }

    pub fn metadata(&self) -> &RawNotebookMetadata {
        &self.raw.metadata
    }

    /// Check if it's a Python notebook.
    ///
    /// This is determined by checking the `language_info` or `kernelspec` in the notebook
    /// metadata. If neither is present, it's assumed to be a Python notebook.
    pub fn is_python_notebook(&self) -> bool {
        if let Some(language_info) = self.raw.metadata.language_info.as_ref() {
            return language_info.name == "python";
        }
        if let Some(kernel_spec) = self.raw.metadata.kernelspec.as_ref() {
            return kernel_spec.language.as_deref() == Some("python");
        }
        true
    }

    /// Write the notebook back to the given [`Write`] implementer.
    pub fn write(&self, writer: &mut dyn Write) -> Result<(), NotebookError> {
        // https://github.com/psf/black/blob/69ca0a4c7a365c5f5eea519a90980bab72cab764/src/black/__init__.py#LL1041
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut serializer = serde_json::Serializer::with_formatter(writer, formatter);
        SortAlphabetically(&self.raw)
            .serialize(&mut serializer)
            .map_err(NotebookError::Json)?;
        if self.trailing_newline {
            writeln!(serializer.into_inner())?;
        }
        Ok(())
    }
}

impl PartialEq for Notebook {
    fn eq(&self, other: &Self) -> bool {
        self.trailing_newline == other.trailing_newline && self.raw == other.raw
    }
}

impl Eq for Notebook {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use ruff_diagnostics::SourceMap;
    use ruff_source_file::OneIndexed;
    use ruff_text_size::TextSize;

    use crate::{Cell, CellStart, Notebook, NotebookError, NotebookIndex};

    /// Construct a path to a Jupyter notebook in the `resources/test/fixtures/jupyter` directory.
    fn notebook_path(path: impl AsRef<Path>) -> std::path::PathBuf {
        Path::new("./resources/test/fixtures/jupyter").join(path)
    }

    #[test_case("valid.ipynb", true)]
    #[test_case("R.ipynb", false)]
    #[test_case("kernelspec_language.ipynb", true)]
    fn is_python_notebook(filename: &str, expected: bool) {
        let notebook = Notebook::from_path(&notebook_path(filename)).unwrap();
        assert_eq!(notebook.is_python_notebook(), expected);
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

    #[test]
    fn empty_notebook() {
        let notebook = Notebook::empty();

        assert_eq!(notebook.source_code(), "\n");
    }

    #[test_case("markdown", false)]
    #[test_case("only_magic", true)]
    #[test_case("code_and_magic", true)]
    #[test_case("only_code", true)]
    #[test_case("cell_magic", false)]
    #[test_case("valid_cell_magic", true)]
    #[test_case("automagic", false)]
    #[test_case("automagic_assignment", true)]
    #[test_case("automagics", false)]
    #[test_case("automagic_before_code", false)]
    #[test_case("automagic_after_code", true)]
    #[test_case("unicode_magic_gh9145", true)]
    #[test_case("vscode_language_id_python", true)]
    #[test_case("vscode_language_id_javascript", false)]
    fn test_is_valid_python_code_cell(cell: &str, expected: bool) -> Result<()> {
        /// Read a Jupyter cell from the `resources/test/fixtures/jupyter/cell` directory.
        fn read_jupyter_cell(path: impl AsRef<Path>) -> Result<Cell> {
            let path = notebook_path("cell").join(path);
            let source_code = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&source_code)?)
        }

        assert_eq!(
            read_jupyter_cell(format!("{cell}.json"))?.is_valid_python_code_cell(),
            expected
        );
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
                cell_starts: vec![
                    CellStart {
                        start_row: OneIndexed::MIN,
                        raw_cell_index: OneIndexed::MIN
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(6),
                        raw_cell_index: OneIndexed::from_zero_indexed(2)
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(11),
                        raw_cell_index: OneIndexed::from_zero_indexed(4)
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(12),
                        raw_cell_index: OneIndexed::from_zero_indexed(6)
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(14),
                        raw_cell_index: OneIndexed::from_zero_indexed(7)
                    }
                ],
            }
        );
        assert_eq!(
            notebook.cell_offsets().as_ref(),
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

    #[test]
    fn index_fragmented_source_array() -> Result<(), NotebookError> {
        let notebook = Notebook::from_source_code(
            r##"{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["p", "a", "s", "s", " ", " ", " "]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["# snapshot\n", "x = 1"]
  }
 ],
 "metadata": {},
 "nbformat": 4,
 "nbformat_minor": 4
}"##,
        )?;

        assert_eq!(notebook.source_code(), "pass   \n# snapshot\nx = 1\n");
        assert_eq!(
            notebook.index(),
            &NotebookIndex {
                cell_starts: vec![
                    CellStart {
                        start_row: OneIndexed::MIN,
                        raw_cell_index: OneIndexed::MIN,
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(1),
                        raw_cell_index: OneIndexed::from_zero_indexed(1),
                    },
                ],
            }
        );

        Ok(())
    }

    #[test]
    fn update_restores_separators_for_empty_cells() -> Result<(), NotebookError> {
        let mut notebook = Notebook::from_source_code(
            r##"{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["import os"]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["import sys"]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["x = 1"]
  }
 ],
 "metadata": {},
 "nbformat": 4,
 "nbformat_minor": 4
}"##,
        )?;

        let mut source_map = SourceMap::default();
        source_map.push_marker(0.into(), 0.into());
        source_map.push_marker(10.into(), 0.into());
        source_map.push_marker(21.into(), 0.into());
        notebook.update(&source_map, "x = 1\n".to_string());

        assert_eq!(notebook.source_code(), "\n\nx = 1\n");
        assert_eq!(
            notebook.cell_offsets().as_ref(),
            &[0.into(), 1.into(), 2.into(), 8.into()]
        );
        assert_eq!(
            notebook.index(),
            &NotebookIndex {
                cell_starts: vec![
                    CellStart {
                        start_row: OneIndexed::MIN,
                        raw_cell_index: OneIndexed::MIN,
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(1),
                        raw_cell_index: OneIndexed::from_zero_indexed(1),
                    },
                    CellStart {
                        start_row: OneIndexed::from_zero_indexed(2),
                        raw_cell_index: OneIndexed::from_zero_indexed(2),
                    },
                ],
            }
        );

        Ok(())
    }

    fn two_cell_notebook() -> Result<Notebook, NotebookError> {
        Notebook::from_source_code(
            r##"{
 "cells": [
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["x = 1"]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": ["x.method(inplace=True)"]
  }
 ],
 "metadata": {},
 "nbformat": 4,
 "nbformat_minor": 4
}"##,
        )
    }

    #[test]
    fn update_keeps_insertion_at_cell_start_in_that_cell() -> Result<(), NotebookError> {
        let mut notebook = two_cell_notebook()?;

        let mut source_map = SourceMap::default();
        source_map.push_marker(6.into(), 6.into());
        source_map.push_marker(6.into(), 10.into());
        notebook.update(
            &source_map,
            "x = 1\nx = x.method(inplace=True)\n".to_string(),
        );

        assert_eq!(
            notebook.source_code(),
            "x = 1\nx = x.method(inplace=True)\n"
        );
        assert_eq!(
            notebook.cell_offsets().as_ref(),
            &[0.into(), 6.into(), 33.into()]
        );

        Ok(())
    }

    #[test]
    fn update_keeps_insertion_at_end_in_non_final_cell() -> Result<(), NotebookError> {
        let mut notebook = two_cell_notebook()?;

        let mut source_map = SourceMap::default();
        source_map.push_marker(5.into(), 5.into());
        source_map.push_marker(5.into(), 16.into());
        notebook.update(
            &source_map,
            "x = 1  # comment\nx.method(inplace=True)\n".to_string(),
        );

        assert_eq!(
            notebook.source_code(),
            "x = 1  # comment\nx.method(inplace=True)\n"
        );
        assert_eq!(
            notebook.cell_offsets().as_ref(),
            &[0.into(), 17.into(), 40.into()]
        );

        Ok(())
    }

    #[test]
    fn update_keeps_insertion_at_end_in_last_cell() {
        let mut notebook = Notebook::empty();
        let end = TextSize::of(notebook.source_code());
        let insertion = "# comment\n";
        let transformed = format!("{}{insertion}", notebook.source_code());

        let mut source_map = SourceMap::default();
        source_map.push_marker(end, end);
        source_map.push_marker(end, end + TextSize::of(insertion));
        notebook.update(&source_map, transformed.clone());

        assert_eq!(
            notebook.cell_offsets().last().copied(),
            Some(TextSize::of(&transformed))
        );
        assert_eq!(notebook.cells()[0].source().to_string(), "\n# comment");
    }

    #[test_case("vscode_language_id.ipynb")]
    #[test_case("kernelspec_language.ipynb")]
    fn round_trip(filename: &str) {
        let path = notebook_path(filename);
        let expected = std::fs::read_to_string(&path).unwrap();
        let actual = super::round_trip(&path).unwrap();
        assert_eq!(actual, expected);
    }
}
