use super::{Document, PositionEncoding};
use lsp_types as types;
use ruff_source_file::OneIndexed;
use ruff_source_file::{LineIndex, SourceLocation};
use ruff_text_size::{TextRange, TextSize};

/// Returns the [`TextRange`] for a LSP [`Range`] respecting the negotiated [`PositionEncoding`].
pub(crate) fn text_range(
    range: types::Range,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> TextRange {
    let start_line = index.line_range(
        OneIndexed::from_zero_indexed(range.start.line as usize),
        text,
    );
    let end_line = index.line_range(OneIndexed::from_zero_indexed(range.end.line as usize), text);

    let (start_column_offset, end_column_offset) = match encoding {
        PositionEncoding::UTF8 => (
            TextSize::new(range.start.character),
            TextSize::new(range.end.character),
        ),

        PositionEncoding::UTF16 => {
            // Fast path for ASCII only documents
            if index.is_ascii() {
                (
                    TextSize::new(range.start.character),
                    TextSize::new(range.end.character),
                )
            } else {
                // UTF16 encodes characters either as one or two 16 bit words.
                // The position in `range` is the 16-bit word offset from the start of the line (and not the character offset)
                // UTF-16 with a text that may use variable-length characters.
                (
                    utf16_column_offset(range.start.character, &text[start_line]),
                    utf16_column_offset(range.end.character, &text[end_line]),
                )
            }
        }
        PositionEncoding::UTF32 => {
            // UTF-32 uses 4 bytes for each character. Meaning, the position in range is a character offset.
            return TextRange::new(
                index.offset(
                    OneIndexed::from_zero_indexed(range.start.line as usize),
                    OneIndexed::from_zero_indexed(range.start.character as usize),
                    text,
                ),
                index.offset(
                    OneIndexed::from_zero_indexed(range.end.line as usize),
                    OneIndexed::from_zero_indexed(range.end.character as usize),
                    text,
                ),
            );
        }
    };

    TextRange::new(
        start_line.start() + start_column_offset.clamp(TextSize::new(0), start_line.end()),
        end_line.start() + end_column_offset.clamp(TextSize::new(0), end_line.end()),
    )
}

fn utf16_column_offset(character: u32, line: &str) -> TextSize {
    let mut character_offset = TextSize::new(0);

    let mut i = 0u32;

    for c in line.chars() {
        if i >= character {
            break;
        }

        // Count characters encoded as two 16 bit words as 2 characters.
        // SAFETY: Value is always between 1 and 2, casting to u32 is safe.
        #[allow(clippy::cast_possible_truncation)]
        {
            character_offset += TextSize::new(c.len_utf8() as u32);
            i += c.len_utf16() as u32;
        }
    }

    character_offset
}

pub(crate) fn offset_to_position(
    offset: TextSize,
    text: &str,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> types::Position {
    let location = match encoding {
        PositionEncoding::UTF8 => {
            let row = index.line_index(offset);
            let column = offset - index.line_start(row, text);

            SourceLocation {
                column: OneIndexed::from_zero_indexed(column.to_usize()),
                row,
            }
        }
        PositionEncoding::UTF16 => {
            let row = index.line_index(offset);

            let column = if index.is_ascii() {
                (offset - index.line_start(row, text)).to_usize()
            } else {
                let up_to_line = &text[TextRange::new(index.line_start(row, text), offset)];
                up_to_line.encode_utf16().count()
            };

            SourceLocation {
                column: OneIndexed::from_zero_indexed(column),
                row,
            }
        }
        PositionEncoding::UTF32 => index.source_location(offset, text),
    };

    #[allow(clippy::cast_possible_truncation)]
    types::Position {
        line: location.row.to_zero_indexed() as u32,
        character: location.column.to_zero_indexed() as u32,
    }
}

pub(crate) fn text_range_to_range(
    text_range: TextRange,
    document: &Document,
    encoding: PositionEncoding,
) -> types::Range {
    types::Range {
        start: offset_to_position(
            text_range.start(),
            document.contents(),
            document.index(),
            encoding,
        ),
        end: offset_to_position(
            text_range.end(),
            document.contents(),
            document.index(),
            encoding,
        ),
    }
}
