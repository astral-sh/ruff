use super::PositionEncoding;
use super::notebook;
use crate::system::file_to_url;

use lsp_types as types;
use lsp_types::Location;

use ruff_db::files::{File, FileRange};
use ruff_db::source::{line_index, source_text};
use ruff_notebook::NotebookIndex;
use ruff_source_file::LineIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_semantic::Db;

#[expect(dead_code)]
pub(crate) struct NotebookRange {
    pub(crate) cell: notebook::CellId,
    pub(crate) range: types::Range,
}

/// Represents a range that has been prepared for LSP conversion but requires
/// a decision about how to use it - either as a local range within the same
/// document/cell, or as a location that can reference any document in the project.
#[derive(Clone)]
pub(crate) struct LspRange<'db> {
    file: File,
    range: TextRange,
    db: &'db dyn Db,
    encoding: PositionEncoding,
}

impl std::fmt::Debug for LspRange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspRange")
            .field("range", &self.range)
            .field("file", &self.file)
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl<'db> LspRange<'db> {
    /// Convert to an LSP Range for use within the same document/cell.
    /// Returns only the LSP Range without any URI information.
    ///
    /// Use this when you already have a URI context and this range is guaranteed
    /// to be within the same document/cell:
    /// - Selection ranges within a LocationLink (where target_uri provides context)
    /// - Additional ranges in the same cell (e.g., selection_range when you already have target_range)
    ///
    /// Do NOT use this for standalone ranges - use `to_location()` instead to ensure
    /// the URI and range are consistent (especially important for notebook cells).
    pub(crate) fn to_local_range(&self) -> types::Range {
        let source = source_text(self.db, self.file);
        let index = line_index(self.db, self.file);
        text_range_to_lsp_range(self.range, &source, &index, self.encoding)
    }

    /// Convert to a Location that can reference any document.
    /// Returns a Location with both URI and Range.
    ///
    /// Use this for:
    /// - Go-to-definition targets
    /// - References
    /// - Diagnostics related information
    /// - Any cross-file navigation
    pub(crate) fn to_location(&self) -> Option<Location> {
        // TODO: Account for the offset of the cell if `file` is a notebook
        // and resolve the URL to the cell's URI.
        let uri = file_to_url(self.db, self.file)?;
        let source = source_text(self.db, self.file);
        let index = line_index(self.db, self.file);
        let range = text_range_to_lsp_range(self.range, &source, &index, self.encoding);
        Some(Location { uri, range })
    }
}

/// Represents a position that has been prepared for LSP conversion but requires
/// a decision about how to use it - either as a local position within the same
/// document/cell, or as a location with a single-point range that can reference
/// any document in the project.
#[derive(Clone)]
pub(crate) struct LspPosition<'db> {
    file: File,
    position: TextSize,
    db: &'db dyn Db,
    encoding: PositionEncoding,
}

impl std::fmt::Debug for LspPosition<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspPosition")
            .field("position", &self.position)
            .field("file", &self.file)
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl<'db> LspPosition<'db> {
    /// Convert to an LSP Position for use within the same document/cell.
    /// Returns only the LSP Position without any URI information.
    ///
    /// Use this when you already have a URI context and this position is guaranteed
    /// to be within the same document/cell:
    /// - Inlay hints (where the document URI is already known)
    /// - Positions within the same cell as a parent range
    ///
    /// Do NOT use this for standalone positions that might need a URI - use
    /// `to_location()` instead to ensure the URI and position are consistent
    /// (especially important for notebook cells).
    pub(crate) fn to_local_position(&self) -> types::Position {
        let source = source_text(self.db, self.file);
        let index = line_index(self.db, self.file);
        text_size_to_lsp_position(self.position, &source, &index, self.encoding)
    }

    /// Convert to a Location with a single-point range that can reference any document.
    /// Returns a Location with both URI and a range where start == end.
    ///
    /// Use this for any cross-file navigation where you need both URI and position.
    #[expect(unused)]
    pub(crate) fn to_location(&self) -> Option<Location> {
        // TODO: Account for the offset of the cell if `file` is a notebook
        // and resolve the URL to the cell's URI.
        let uri = file_to_url(self.db, self.file)?;
        let source = source_text(self.db, self.file);
        let index = line_index(self.db, self.file);
        let position = text_size_to_lsp_position(self.position, &source, &index, self.encoding);
        Some(Location {
            uri,
            range: types::Range {
                start: position,
                end: position,
            },
        })
    }
}

pub(crate) trait RangeExt {
    /// Convert an LSP Range to internal TextRange.
    ///
    /// For notebook support, the caller should have the URI to determine which cell
    /// the range refers to, and pass the corresponding file.
    fn to_text_range(&self, db: &dyn Db, file: File, encoding: PositionEncoding) -> TextRange;
}

impl RangeExt for lsp_types::Range {
    fn to_text_range(&self, db: &dyn Db, file: File, encoding: PositionEncoding) -> TextRange {
        // TODO: For notebooks, lookup the offset in the right cell.
        let source = source_text(db, file);
        let index = line_index(db, file);
        lsp_range_to_text_range(self, &source, &index, encoding)
    }
}

pub(crate) trait PositionExt {
    /// Convert an LSP Position to internal TextSize.
    ///
    /// TODO: For notebook support, this will need `db` and `file` parameters to:
    /// - Determine if the file is a notebook
    /// - Map cell-relative position to absolute position in the notebook file
    /// - Note: The caller also needs access to the URI to determine which cell
    fn to_text_size(&self, text: &str, index: &LineIndex, encoding: PositionEncoding) -> TextSize;
}

impl PositionExt for lsp_types::Position {
    fn to_text_size(&self, text: &str, index: &LineIndex, encoding: PositionEncoding) -> TextSize {
        lsp_position_to_text_size(self, text, index, encoding)
    }
}

pub(crate) trait TextSizeExt {
    /// Converts this position to an `LspPosition`, which then requires an explicit
    /// decision about how to use it (as a local position or as a location).
    fn to_lsp_position<'db>(
        self,
        db: &'db dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> LspPosition<'db>
    where
        Self: Sized;
}

impl TextSizeExt for TextSize {
    fn to_lsp_position<'db>(
        self,
        db: &'db dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> LspPosition<'db> {
        LspPosition {
            file,
            position: self,
            db,
            encoding,
        }
    }
}

pub(crate) trait ToRangeExt {
    /// Converts this range to an `LspRange`, which then requires an explicit
    /// decision about how to use it (as a local range or as a location).
    fn as_lsp_range<'db>(
        &self,
        db: &'db dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> LspRange<'db>;

    #[expect(dead_code)]
    fn to_notebook_range(
        &self,
        text: &str,
        source_index: &LineIndex,
        notebook_index: &NotebookIndex,
        encoding: PositionEncoding,
    ) -> NotebookRange;
}

fn u32_index_to_usize(index: u32) -> usize {
    usize::try_from(index).expect("u32 fits in usize")
}

fn text_size_to_lsp_position(
    offset: TextSize,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> types::Position {
    let source_location = index.source_location(offset, text, encoding.into());
    source_location_to_position(&source_location)
}

fn text_range_to_lsp_range(
    range: TextRange,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> types::Range {
    types::Range {
        start: text_size_to_lsp_position(range.start(), text, index, encoding),
        end: text_size_to_lsp_position(range.end(), text, index, encoding),
    }
}

/// Helper function to convert an LSP Position to internal TextSize.
/// This is used internally by the `PositionExt` trait and other helpers.
fn lsp_position_to_text_size(
    position: &lsp_types::Position,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> TextSize {
    index.offset(
        SourceLocation {
            line: OneIndexed::from_zero_indexed(u32_index_to_usize(position.line)),
            character_offset: OneIndexed::from_zero_indexed(u32_index_to_usize(position.character)),
        },
        text,
        encoding.into(),
    )
}

/// Helper function to convert an LSP Range to internal TextRange.
/// This is used internally by the `RangeExt` trait and in special cases
/// where `db` and `file` are not available (e.g., when applying document changes).
pub(crate) fn lsp_range_to_text_range(
    range: &lsp_types::Range,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> TextRange {
    TextRange::new(
        lsp_position_to_text_size(&range.start, text, index, encoding),
        lsp_position_to_text_size(&range.end, text, index, encoding),
    )
}

impl ToRangeExt for TextRange {
    fn as_lsp_range<'db>(
        &self,
        db: &'db dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> LspRange<'db> {
        LspRange {
            file,
            range: *self,
            db,
            encoding,
        }
    }

    fn to_notebook_range(
        &self,
        text: &str,
        source_index: &LineIndex,
        notebook_index: &NotebookIndex,
        encoding: PositionEncoding,
    ) -> NotebookRange {
        let start = source_index.source_location(self.start(), text, encoding.into());
        let mut end = source_index.source_location(self.end(), text, encoding.into());
        let starting_cell = notebook_index.cell(start.line);

        // weird edge case here - if the end of the range is where the newline after the cell got added (making it 'out of bounds')
        // we need to move it one character back (which should place it at the end of the last line).
        // we test this by checking if the ending offset is in a different (or nonexistent) cell compared to the cell of the starting offset.
        if notebook_index.cell(end.line) != starting_cell {
            end.line = end.line.saturating_sub(1);
            let offset = self.end().checked_sub(1.into()).unwrap_or_default();
            end.character_offset = source_index
                .source_location(offset, text, encoding.into())
                .character_offset;
        }

        let start = source_location_to_position(&notebook_index.translate_source_location(&start));
        let end = source_location_to_position(&notebook_index.translate_source_location(&end));

        NotebookRange {
            cell: starting_cell
                .map(OneIndexed::to_zero_indexed)
                .unwrap_or_default(),
            range: types::Range { start, end },
        }
    }
}

fn source_location_to_position(location: &SourceLocation) -> types::Position {
    types::Position {
        line: u32::try_from(location.line.to_zero_indexed()).expect("line usize fits in u32"),
        character: u32::try_from(location.character_offset.to_zero_indexed())
            .expect("character usize fits in u32"),
    }
}

pub(crate) trait FileRangeExt {
    /// Converts this file range to an `LspRange`, which then requires an explicit
    /// decision about how to use it (as a local range or as a location).
    fn as_lsp_range<'db>(&self, db: &'db dyn Db, encoding: PositionEncoding) -> LspRange<'db>;
}

impl FileRangeExt for FileRange {
    fn as_lsp_range<'db>(&self, db: &'db dyn Db, encoding: PositionEncoding) -> LspRange<'db> {
        LspRange {
            file: self.file(),
            range: self.range(),
            db,
            encoding,
        }
    }
}
