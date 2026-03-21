use super::PositionEncoding;
use crate::Db;
use crate::system::file_to_url;

use ruff_db::files::{File, FileRange};
use ruff_db::source::{line_index, source_text};
use ruff_source_file::LineIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// A range in an LSP text document (cell or a regular document).
#[derive(Clone, Debug, Default)]
pub(crate) struct LspRange {
    range: lsp_types::Range,

    /// The URI of this range's text document
    uri: Option<lsp_types::Url>,
}

impl LspRange {
    /// Returns the range within this document.
    ///
    /// Only use `range` when you already have a URI context and this range is guaranteed
    /// to be within the same document/cell:
    /// - Selection ranges within a `LocationLink` (where `target_uri` provides context)
    /// - Additional ranges in the same cell (e.g., `selection_range` when you already have `target_range`)
    ///
    /// Do NOT use this for standalone ranges - use [`Self::to_location`] instead to ensure
    /// the URI and range are consistent.
    pub(crate) fn local_range(&self) -> lsp_types::Range {
        self.range
    }

    /// Converts this range into an LSP location.
    ///
    /// Returns `None` if the URI for this file couldn't be resolved.
    pub(crate) fn to_location(&self) -> Option<lsp_types::Location> {
        Some(lsp_types::Location {
            uri: self.uri.clone()?,
            range: self.range,
        })
    }

    pub(crate) fn into_location(self) -> Option<lsp_types::Location> {
        Some(lsp_types::Location {
            uri: self.uri?,
            range: self.range,
        })
    }
}

/// A position in an LSP text document (cell or a regular document).
#[derive(Clone, Debug, Default)]
pub(crate) struct LspPosition {
    position: lsp_types::Position,

    /// The URI of this range's text document
    uri: Option<lsp_types::Url>,
}

impl LspPosition {
    /// Returns the position within this document.
    ///
    /// Only use [`Self::local_position`] when you already have a URI context and this position is guaranteed
    /// to be within the same document/cell
    ///
    /// Do NOT use this for standalone positions - use [`Self::to_location`] instead to ensure
    /// the URI and position are consistent.
    pub(crate) fn local_position(&self) -> lsp_types::Position {
        self.position
    }

    /// Returns the uri of the text document this position belongs to.
    #[expect(unused)]
    pub(crate) fn uri(&self) -> Option<&lsp_types::Url> {
        self.uri.as_ref()
    }
}

pub(crate) trait RangeExt {
    /// Convert an LSP Range to a [`TextRange`].
    ///
    /// Returns `None` if `file` is a notebook and the
    /// cell identified by `url` can't be looked up or if the notebook
    /// isn't open in the editor.
    fn to_text_range(
        &self,
        db: &dyn Db,
        file: File,
        url: &lsp_types::Url,
        encoding: PositionEncoding,
    ) -> Option<TextRange>;
}

impl RangeExt for lsp_types::Range {
    fn to_text_range(
        &self,
        db: &dyn Db,
        file: File,
        url: &lsp_types::Url,
        encoding: PositionEncoding,
    ) -> Option<TextRange> {
        let start = self.start.to_text_size(db, file, url, encoding)?;
        let end = self.end.to_text_size(db, file, url, encoding)?;

        Some(TextRange::new(start, end))
    }
}

pub(crate) trait PositionExt {
    /// Convert an LSP Position to internal `TextSize`.
    ///
    /// For notebook support, this uses the URI to determine which cell the position
    /// refers to, and maps the cell-relative position to the absolute position in the
    /// concatenated notebook file.
    ///
    /// Returns `None` if `file` is a notebook and the
    /// cell identified by `url` can't be looked up or if the notebook
    /// isn't open in the editor.
    fn to_text_size(
        &self,
        db: &dyn Db,
        file: File,
        url: &lsp_types::Url,
        encoding: PositionEncoding,
    ) -> Option<TextSize>;
}

impl PositionExt for lsp_types::Position {
    fn to_text_size(
        &self,
        db: &dyn Db,
        file: File,
        url: &lsp_types::Url,
        encoding: PositionEncoding,
    ) -> Option<TextSize> {
        let source = source_text(db, file);
        let index = line_index(db, file);

        if let Some(notebook) = source.as_notebook() {
            let notebook_document = db.notebook_document(file)?;
            let cell_index = notebook_document.cell_index_by_uri(url)?;

            let cell_start_offset = notebook.cell_offset(cell_index).unwrap_or_default();
            let cell_relative_line = OneIndexed::from_zero_indexed(u32_index_to_usize(self.line));

            let cell_start_location =
                index.source_location(cell_start_offset, source.as_str(), encoding.into());
            assert_eq!(cell_start_location.character_offset, OneIndexed::MIN);

            // Absolute position into the concatenated notebook source text.
            let absolute_position = SourceLocation {
                line: cell_start_location
                    .line
                    .saturating_add(cell_relative_line.to_zero_indexed()),
                character_offset: OneIndexed::from_zero_indexed(u32_index_to_usize(self.character)),
            };
            return Some(index.offset(absolute_position, &source, encoding.into()));
        }

        Some(lsp_position_to_text_size(*self, &source, &index, encoding))
    }
}

pub(crate) trait TextSizeExt {
    /// Converts `self` into a position in an LSP text document (can be a cell or regular document).
    ///
    /// Returns `None` if the position can't be converted:
    ///
    /// * If `file` is a notebook but the notebook isn't open in the editor,
    ///   preventing us from looking up the corresponding cell.
    /// * If `position` is out of bounds.
    fn to_lsp_position(
        &self,
        db: &dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> Option<LspPosition>
    where
        Self: Sized;
}

impl TextSizeExt for TextSize {
    fn to_lsp_position(
        &self,
        db: &dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> Option<LspPosition> {
        let source = source_text(db, file);
        let index = line_index(db, file);

        if let Some(notebook) = source.as_notebook() {
            let notebook_document = db.notebook_document(file)?;
            let start = index.source_location(*self, source.as_str(), encoding.into());
            let cell = notebook.index().cell(start.line)?;

            let cell_relative_start = notebook.index().translate_source_location(&start);

            return Some(LspPosition {
                uri: Some(notebook_document.cell_uri_by_index(cell)?.clone()),
                position: source_location_to_position(&cell_relative_start),
            });
        }

        let uri = file_to_url(db, file);
        let position = text_size_to_lsp_position(*self, &source, &index, encoding);

        Some(LspPosition { position, uri })
    }
}

pub(crate) trait ToRangeExt {
    /// Converts self into a range into an LSP text document (can be a cell or regular document).
    ///
    /// Returns `None` if the range can't be converted:
    ///
    /// * If `file` is a notebook but the notebook isn't open in the editor,
    ///   preventing us from looking up the corresponding cell.
    /// * If range is out of bounds.
    fn to_lsp_range(&self, db: &dyn Db, file: File, encoding: PositionEncoding)
    -> Option<LspRange>;
}

fn u32_index_to_usize(index: u32) -> usize {
    usize::try_from(index).expect("u32 fits in usize")
}

fn text_size_to_lsp_position(
    offset: TextSize,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> lsp_types::Position {
    let source_location = index.source_location(offset, text, encoding.into());
    source_location_to_position(&source_location)
}

fn text_range_to_lsp_range(
    range: TextRange,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> lsp_types::Range {
    lsp_types::Range {
        start: text_size_to_lsp_position(range.start(), text, index, encoding),
        end: text_size_to_lsp_position(range.end(), text, index, encoding),
    }
}

/// Helper function to convert an LSP Position to internal `TextSize`.
/// This is used internally by the `PositionExt` trait and other helpers.
fn lsp_position_to_text_size(
    position: lsp_types::Position,
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

/// Helper function to convert an LSP Range to internal `TextRange`.
/// This is used internally by the `RangeExt` trait and in special cases
/// where `db` and `file` are not available (e.g., when applying document changes).
pub(crate) fn lsp_range_to_text_range(
    range: lsp_types::Range,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> TextRange {
    TextRange::new(
        lsp_position_to_text_size(range.start, text, index, encoding),
        lsp_position_to_text_size(range.end, text, index, encoding),
    )
}

impl ToRangeExt for TextRange {
    fn to_lsp_range(
        &self,
        db: &dyn Db,
        file: File,
        encoding: PositionEncoding,
    ) -> Option<LspRange> {
        let source = source_text(db, file);
        let index = line_index(db, file);

        if let Some(notebook) = source.as_notebook() {
            let notebook_index = notebook.index();
            let notebook_document = db.notebook_document(file)?;

            let start_in_concatenated =
                index.source_location(self.start(), &source, encoding.into());
            let cell_index = notebook_index.cell(start_in_concatenated.line)?;

            let end_in_concatenated = index.source_location(self.end(), &source, encoding.into());

            let start_in_cell = source_location_to_position(
                &notebook_index.translate_source_location(&start_in_concatenated),
            );
            let end_in_cell = source_location_to_position(
                &notebook_index.translate_source_location(&end_in_concatenated),
            );

            let cell_uri = notebook_document
                .cell_uri_by_index(cell_index)
                .expect("Index to contain an URI for every cell");

            return Some(LspRange {
                uri: Some(cell_uri.clone()),
                range: lsp_types::Range::new(start_in_cell, end_in_cell),
            });
        }

        let range = text_range_to_lsp_range(*self, &source, &index, encoding);

        let uri = file_to_url(db, file);
        Some(LspRange { range, uri })
    }
}

fn source_location_to_position(location: &SourceLocation) -> lsp_types::Position {
    lsp_types::Position {
        line: u32::try_from(location.line.to_zero_indexed()).expect("line usize fits in u32"),
        character: u32::try_from(location.character_offset.to_zero_indexed())
            .expect("character usize fits in u32"),
    }
}

pub(crate) trait FileRangeExt {
    /// Converts this file range to an `LspRange`, which then requires an explicit
    /// decision about how to use it (as a local range or as a location).
    fn to_lsp_range(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<LspRange>;
}

impl FileRangeExt for FileRange {
    fn to_lsp_range(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<LspRange> {
        self.range().to_lsp_range(db, self.file(), encoding)
    }
}
