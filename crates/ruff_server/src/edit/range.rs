use super::PositionEncoding;
use super::notebook;
use lsp_types as types;
use ruff_notebook::NotebookIndex;
use ruff_source_file::LineIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::{TextRange, TextSize};

pub(crate) struct NotebookRange {
    pub(crate) cell: notebook::CellId,
    pub(crate) range: types::Range,
}

pub(crate) trait RangeExt {
    fn to_text_range(
        &self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> Result<TextRange, PositionError>;
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

#[derive(Debug, thiserror::Error)]
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

impl RangeExt for lsp_types::Range {
    fn to_text_range(
        &self,
        text: &str,
        index: &LineIndex,
        encoding: PositionEncoding,
    ) -> Result<TextRange, PositionError> {
        let start = position_to_offset(self.start, text, index, encoding)?;
        let end = position_to_offset(self.end, text, index, encoding)?;

        if start > end {
            return Err(PositionError::ReversedRange);
        }

        Ok(TextRange::new(start, end))
    }
}

fn position_to_offset(
    position: types::Position,
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
    let line_end = index.line_end_exclusive(line, text);
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

fn offset_for_encoded_character(
    text: &str,
    character: usize,
    encoded_len: impl Fn(char) -> usize,
    position: types::Position,
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
