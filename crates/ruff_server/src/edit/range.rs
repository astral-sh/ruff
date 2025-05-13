use super::notebook;
use super::PositionEncoding;
use lsp_types as types;
use ruff_notebook::NotebookIndex;
use ruff_source_file::LineIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::TextRange;

pub(crate) struct NotebookRange {
    pub(crate) cell: notebook::CellId,
    pub(crate) range: types::Range,
}

pub(crate) trait RangeExt {
    fn to_text_range(&self, text: &str, index: &LineIndex, encoding: PositionEncoding)
        -> TextRange;
}

pub(crate) trait ToRangeExt {
    fn to_range(&self, text: &str, index: &LineIndex, encoding: PositionEncoding) -> types::Range;
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

impl RangeExt for lsp_types::Range {
    fn to_text_range(
        &self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> TextRange {
        let start = index.offset(
            SourceLocation {
                line: OneIndexed::from_zero_indexed(u32_index_to_usize(self.start.line)),
                character_offset: OneIndexed::from_zero_indexed(u32_index_to_usize(
                    self.start.character,
                )),
            },
            text,
            encoding.into(),
        );
        let end = index.offset(
            SourceLocation {
                line: OneIndexed::from_zero_indexed(u32_index_to_usize(self.end.line)),
                character_offset: OneIndexed::from_zero_indexed(u32_index_to_usize(
                    self.end.character,
                )),
            },
            text,
            encoding.into(),
        );

        TextRange::new(start, end)
    }
}

impl ToRangeExt for TextRange {
    fn to_range(&self, text: &str, index: &LineIndex, encoding: PositionEncoding) -> types::Range {
        types::Range {
            start: source_location_to_position(&index.source_location(
                self.start(),
                text,
                encoding.into(),
            )),
            end: source_location_to_position(&index.source_location(
                self.end(),
                text,
                encoding.into(),
            )),
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
            end.character_offset = source_index
                .source_location(
                    self.end().checked_sub(1.into()).unwrap_or_default(),
                    text,
                    encoding.into(),
                )
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
        line: u32::try_from(location.line.to_zero_indexed()).expect("row usize fits in u32"),
        character: u32::try_from(location.character_offset.to_zero_indexed())
            .expect("character usize fits in u32"),
    }
}
