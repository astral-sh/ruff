use super::notebook;
use super::PositionEncoding;
use crate::system::file_to_url;

use lsp_types as types;
use lsp_types::Location;

use ruff_db::files::FileRange;
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

pub(crate) trait RangeExt {
    fn to_text_range(&self, text: &str, index: &LineIndex, encoding: PositionEncoding)
        -> TextRange;
}

pub(crate) trait PositionExt {
    fn to_text_size(&self, text: &str, index: &LineIndex, encoding: PositionEncoding) -> TextSize;
}

pub(crate) trait TextSizeExt {
    fn to_position(
        self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> types::Position
    where
        Self: Sized;
}

impl TextSizeExt for TextSize {
    fn to_position(
        self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> types::Position {
        let source_location = index.source_location(self, text, encoding.into());
        source_location_to_position(&source_location)
    }
}

pub(crate) trait ToRangeExt {
    fn to_lsp_range(
        &self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> types::Range;

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

impl PositionExt for lsp_types::Position {
    fn to_text_size(&self, text: &str, index: &LineIndex, encoding: PositionEncoding) -> TextSize {
        index.offset(
            SourceLocation {
                line: OneIndexed::from_zero_indexed(u32_index_to_usize(self.line)),
                character_offset: OneIndexed::from_zero_indexed(u32_index_to_usize(self.character)),
            },
            text,
            encoding.into(),
        )
    }
}

impl RangeExt for lsp_types::Range {
    fn to_text_range(
        &self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> TextRange {
        TextRange::new(
            self.start.to_text_size(text, index, encoding),
            self.end.to_text_size(text, index, encoding),
        )
    }
}

impl ToRangeExt for TextRange {
    fn to_lsp_range(
        &self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> types::Range {
        types::Range {
            start: self.start().to_position(text, index, encoding),
            end: self.end().to_position(text, index, encoding),
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
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location>;
}

impl FileRangeExt for FileRange {
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location> {
        let file = self.file();
        let uri = file_to_url(db, file)?;
        let source = source_text(db.upcast(), file);
        let line_index = line_index(db.upcast(), file);

        let range = self.range().to_lsp_range(&source, &line_index, encoding);
        Some(Location { uri, range })
    }
}
