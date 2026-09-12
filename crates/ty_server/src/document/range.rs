use super::PositionEncoding;
use crate::Db;
use crate::system::file_to_uri;

use lsp_types::Uri;
use ruff_db::files::{File, FileRange, system_path_to_file, vendored_path_to_file};
use ruff_db::source::{line_index, source_text};
use ruff_db::system::SystemPathBuf;
use ruff_source_file::LineIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use ty_project::ProjectDatabase;

/// A range in an LSP text document (cell or a regular document).
#[derive(Clone, Debug, Default)]
pub(crate) struct LspRange {
    range: lsp_types::Range,

    /// The URI of this range's text document
    uri: Option<lsp_types::Uri>,
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub(crate) enum PositionError {
    #[error("line {line} is out of bounds")]
    LineOutOfBounds { line: u32 },
    #[error("character {character} is not a valid {encoding:?} position on line {line}")]
    InvalidCharacter {
        line: u32,
        character: u32,
        encoding: PositionEncoding,
    },
    #[error("range start must not be after its end")]
    ReversedRange,
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
    uri: Option<lsp_types::Uri>,
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
    fn uri(&self) -> Option<&lsp_types::Uri> {
        self.uri.as_ref()
    }
}

pub(crate) trait RangeExt {
    /// Convert an LSP Range to a [`TextRange`].
    ///
    /// Returns `None` if `file` is a notebook and the
    /// cell identified by `uri` can't be looked up or if the notebook
    /// isn't open in the editor.
    fn to_text_range(
        &self,
        db: &dyn Db,
        file: File,
        uri: &lsp_types::Uri,
        encoding: PositionEncoding,
    ) -> Option<TextRange>;
}

impl RangeExt for lsp_types::Range {
    fn to_text_range(
        &self,
        db: &dyn Db,
        file: File,
        uri: &lsp_types::Uri,
        encoding: PositionEncoding,
    ) -> Option<TextRange> {
        let start = self.start.to_text_size(db, file, uri, encoding)?;
        let end = self.end.to_text_size(db, file, uri, encoding)?;

        if start > end {
            return None;
        }

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
    /// cell identified by `uri` can't be looked up or if the notebook
    /// isn't open in the editor.
    fn to_text_size(
        &self,
        db: &dyn Db,
        file: File,
        uri: &lsp_types::Uri,
        encoding: PositionEncoding,
    ) -> Option<TextSize>;
}

impl PositionExt for lsp_types::Position {
    fn to_text_size(
        &self,
        db: &dyn Db,
        file: File,
        uri: &lsp_types::Uri,
        encoding: PositionEncoding,
    ) -> Option<TextSize> {
        let source = source_text(db, file);
        let index = line_index(db, file);

        if let Some(notebook) = source.as_notebook() {
            let notebook_document = db.notebook_document(file)?;
            let cell_index = notebook_document.cell_index_by_uri(uri)?;

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

        let uri = file_to_uri(db, file);
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

/// Fallible position to offset conversion for raw LSP client input.
fn try_lsp_position_to_text_size(
    position: lsp_types::Position,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> Result<TextSize, PositionError> {
    let line_index = u32_index_to_usize(position.line);
    if line_index >= index.line_count() {
        return Err(PositionError::LineOutOfBounds {
            line: position.line,
        });
    }

    let line = OneIndexed::from_zero_indexed(line_index);
    let line_start = index.line_start(line, text);
    let line_end = line_end_exclusive(index, line, text);
    let text_on_line = &text[usize::from(line_start)..usize::from(line_end)];
    let character = u32_index_to_usize(position.character);

    let byte_offset = match encoding {
        PositionEncoding::UTF8 => {
            if character > text_on_line.len() || !text_on_line.is_char_boundary(character) {
                return Err(PositionError::InvalidCharacter {
                    line: position.line,
                    character: position.character,
                    encoding,
                });
            }
            character
        }
        PositionEncoding::UTF16 => offset_for_encoded_character(
            text_on_line,
            character,
            char::len_utf16,
            position,
            encoding,
        )?,
        PositionEncoding::UTF32 => {
            offset_for_encoded_character(text_on_line, character, |_| 1, position, encoding)?
        }
    };

    Ok(line_start + TextSize::try_from(byte_offset).expect("line offset fits in TextSize"))
}

fn line_end_exclusive(index: &LineIndex, line: OneIndexed, contents: &str) -> TextSize {
    let row_index = line.to_zero_indexed();
    let starts = index.line_starts();

    if row_index.saturating_add(1) >= starts.len() {
        contents.text_len()
    } else {
        let next_line_start = starts[row_index + 1].to_usize();
        let bytes = contents.as_bytes();

        let line_ending_len = if bytes[..next_line_start].ends_with(b"\r\n") {
            2
        } else {
            1
        };
        starts[row_index + 1] - TextSize::new(line_ending_len)
    }
}

fn offset_for_encoded_character(
    text: &str,
    character: usize,
    encoded_len: impl Fn(char) -> usize,
    position: lsp_types::Position,
    encoding: PositionEncoding,
) -> Result<usize, PositionError> {
    let mut encoded_offset = 0;

    for (byte_offset, current) in text.char_indices() {
        if encoded_offset == character {
            return Ok(byte_offset);
        }
        encoded_offset += encoded_len(current);
    }

    if encoded_offset == character {
        Ok(text.len())
    } else {
        Err(PositionError::InvalidCharacter {
            line: position.line,
            character: position.character,
            encoding,
        })
    }
}

/// Helper function to convert an LSP Range to internal `TextRange`.
/// This is used internally by the `RangeExt` trait and in special cases
/// where `db` and `file` are not available (e.g., when applying document changes).
pub(crate) fn lsp_range_to_text_range(
    range: lsp_types::Range,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> Result<TextRange, PositionError> {
    let start = try_lsp_position_to_text_size(range.start, text, index, encoding)?;
    let end = try_lsp_position_to_text_size(range.end, text, index, encoding)?;

    if start > end {
        return Err(PositionError::ReversedRange);
    }

    Ok(TextRange::new(start, end))
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

        let uri = file_to_uri(db, file);
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

/// Attempts to resolve the location for a file and range. This includes
/// mapping system paths back into their proper vendored
/// path types (if applicable).
pub(crate) fn resolve_file_uri_range(
    db: &ProjectDatabase,
    file_uri: &Uri,
    range: lsp_types::Range,
    encoding: PositionEncoding,
) -> Option<(File, TextSize)> {
    let system_path = SystemPathBuf::from_path_buf(file_uri.to_file_path().ok()?).ok()?;

    let file = if let Some(ref vendored_root) = ty_ide::cached_vendored_root(db)
        && let Some(vendored_path) = ty_ide::map_system_to_vendored(vendored_root, &system_path)
    {
        match vendored_path_to_file(db, vendored_path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(
                    "Could not resolve item location \
                     for vendored file path `{vendored_path}`: {err}"
                );
                return None;
            }
        }
    } else {
        match system_path_to_file(db, &system_path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(
                    "Could not resolve item location \
                     for system file path `{system_path}`: {err}"
                );
                return None;
            }
        }
    };

    let offset = range.start.to_text_size(db, file, file_uri, encoding)?;
    Some((file, offset))
}
