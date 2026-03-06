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

use ruff_diagnostics::SourceMap;
use ruff_source_file::{NewlineWithTrailingNewline, OneIndexed, UniversalNewlineIterator};
use ruff_text_size::{TextRange, TextSize};

use crate::cell::CellOffsets;
use crate::index::NotebookIndex;
use crate::schema::{Cell, RawNotebook, SortAlphabetically, SourceValue};
use crate::{CellMetadata, CellStart, RawNotebookMetadata, schema};

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

        let mut contents = Vec::with_capacity(valid_code_cells.len());
        let mut current_offset = TextSize::from(0);
        let mut cell_offsets = CellOffsets::with_capacity(valid_code_cells.len());
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

        Ok(Self {
            raw: raw_notebook,
            index: OnceLock::new(),
            // The additional newline at the end is to maintain consistency for
            // all cells. These newlines will be removed before updating the
            // source code with the transformed content. Refer `update_cell_content`.
            source_code: contents.join("\n") + "\n",
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

    /// Update the cell offsets as per the given [`SourceMap`].
    fn update_cell_offsets(&mut self, source_map: &SourceMap) {
        // The first offset is always going to be at 0, so skip it.
        for offset in self.cell_offsets.iter_mut().skip(1) {
            let markers = source_map.markers();
            // Find the last marker that starts at or before the current offset.
            let idx = markers.partition_point(|m| m.source() <= *offset);
            if idx == 0 {
                // If there are no markers at or before the current offset, we assume that the
                // offset is unchanged.
                continue;
            }
            let m1_idx = idx - 1;
            let m1 = &markers[m1_idx];

            let mut new_offset = match m1.source().cmp(&m1.dest()) {
                Ordering::Less => *offset + (m1.dest() - m1.source()),
                Ordering::Greater => *offset - (m1.source() - m1.dest()),
                Ordering::Equal => *offset,
            };

            // If there's a next marker, we need to ensure that the new offset doesn't go beyond
            // the start of the next segment. This handles cases where the current segment was
            // shortened or deleted.
            if let Some(m2) = markers.get(m1_idx + 1) {
                if new_offset > m2.dest() {
                    new_offset = m2.dest();
                }
            }

            *offset = new_offset;
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

    /// Build and return the [`NotebookIndex`].
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
        let mut cell_starts = Vec::with_capacity(self.valid_code_cells.len());

        let mut current_row = OneIndexed::MIN;

        for &cell_index in &self.valid_code_cells {
            let raw_cell_index = cell_index as usize;
            // Record the starting row of this cell
            cell_starts.push(CellStart {
                start_row: current_row,
                raw_cell_index: OneIndexed::from_zero_indexed(raw_cell_index),
            });

            let line_count = match &self.raw.cells[raw_cell_index].source() {
                SourceValue::String(string) => {
                    if string.is_empty() {
                        1
                    } else {
                        NewlineWithTrailingNewline::from(string).count()
                    }
                }
                SourceValue::StringArray(string_array) => {
                    if string_array.is_empty() {
                        1
                    } else {
                        let trailing_newline =
                            usize::from(string_array.last().is_some_and(|s| s.ends_with('\n')));
                        string_array.len() + trailing_newline
                    }
                }
            };

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
        self.update_cell_content(&transformed);
        self.source_code = transformed;
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

    use ruff_source_file::OneIndexed;

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

    #[test_case("vscode_language_id.ipynb")]
    #[test_case("kernelspec_language.ipynb")]
    fn round_trip(filename: &str) {
        let path = notebook_path(filename);
        let expected = std::fs::read_to_string(&path).unwrap();
        let actual = super::round_trip(&path).unwrap();
        assert_eq!(actual, expected);
    }

    /// Regression test for <https://github.com/astral-sh/ruff/issues/22797>.
    ///
    /// When applying W391 fix to a notebook with multiple empty cells, the
    /// `update_cell_offsets` function incorrectly calculated cell boundaries
    /// when content was deleted (trailing newlines removed). Offsets for cells
    /// in the affected range would overshoot into deleted regions, causing
    /// invalid `start > end` ranges in `update_cell_content` and a panic.
    ///
    /// The fix clamps each cell's new offset to the destination of the *next*
    /// marker if the calculated offset would exceed it.
    #[test]
    fn update_with_multi_empty_cells() {
        use ruff_diagnostics::SourceMap;

        let mut notebook = Notebook::from_path(&notebook_path("empty_cells.ipynb")).unwrap();

        // The notebook has 5 cells:
        // Cell 0: "\n\n\n" (3 newlines)
        // Cells 1-4: empty

        // Verify initial source and offsets.
        assert_eq!(notebook.source_code(), "\n\n\n\n\n\n\n\n");
        let initial_offsets: Vec<u32> = notebook
            .cell_offsets()
            .iter()
            .map(ruff_text_size::TextSize::to_u32)
            .collect();
        // [0, 4, 5, 6, 7, 8] - each cell separated by \n, first cell has content "\n\n\n"
        assert_eq!(initial_offsets, vec![0, 4, 5, 6, 7, 8]);

        // Create a SourceMap that simulates W391 fix: delete trailing newlines
        // from first cell (delete chars at positions 1, 2, 3 - keeping only first newline).
        //
        // Source: "\n\n\n\n..." (len=8)
        // Target: "\n\n..."    (len=5, deleting 3 chars from first cell)
        //
        // Markers:
        // - (0, 0): start of content, unchanged
        // - (4, 1): after deletion of 3 chars from first cell
        //
        // This simulates content being shortened, which previously caused
        // the panic when multiple cells followed the shortened content.
        let mut source_map = SourceMap::default();
        source_map.push_marker(0.into(), 0.into());
        source_map.push_marker(4.into(), 1.into());

        // The transformed content after the "fix":
        // First cell now only has "\n" instead of "\n\n\n"
        // Overall: "\n" + "\n" + "\n" + "\n" + "\n" = 5 newlines
        let transformed = "\n\n\n\n\n".to_string();

        // This should NOT panic with the fix in place.
        notebook.update(&source_map, transformed.clone());

        // Verify the update succeeded
        assert_eq!(notebook.source_code(), transformed);

        // Verify that offsets are correctly clamped
        let updated_offsets: Vec<u32> = notebook
            .cell_offsets()
            .iter()
            .map(ruff_text_size::TextSize::to_u32)
            .collect();
        // All offsets should be valid (non-decreasing and <= content length)
        for i in 0..updated_offsets.len() - 1 {
            assert!(
                updated_offsets[i] <= updated_offsets[i + 1],
                "Offsets should be non-decreasing: {updated_offsets:?}",
            );
        }
        let content_len = u32::try_from(transformed.len()).unwrap();
        assert!(
            *updated_offsets.last().unwrap() <= content_len,
            "Last offset should not exceed content length"
        );
    }
}
