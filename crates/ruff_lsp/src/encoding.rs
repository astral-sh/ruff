use similar::{ChangeTag, TextDiff};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU8, Ordering};

use tower_lsp::lsp_types::{Position, PositionEncodingKind, Range, TextEdit};

use crate::document::Document;
use ruff_source_file::{LineIndex, OneIndexed, SourceLocation};
use ruff_text_size::{TextLen, TextRange, TextSize};

/// Returns the [`TextRange`] for a LSP [`Range`] respecting the negotiated [`PositionEncoding`]
pub(crate) fn text_range(
    range: Range,
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
    line_index: &LineIndex,
    encoding: PositionEncoding,
) -> Position {
    let location = match encoding {
        PositionEncoding::UTF8 => {
            let row = line_index.line_index(offset);
            let column = offset - line_index.line_start(row, text);

            SourceLocation {
                column: OneIndexed::from_zero_indexed(column.to_usize()),
                row,
            }
        }
        PositionEncoding::UTF16 => {
            let row = line_index.line_index(offset);
            let line_start = line_index.line_start(row, text);

            let column = if line_index.is_ascii() {
                (offset - line_start).to_usize()
            } else {
                let up_to_line = &text[TextRange::new(line_index.line_start(row, text), offset)];
                up_to_line.encode_utf16().count()
            };

            SourceLocation {
                column: OneIndexed::from_zero_indexed(column),
                row,
            }
        }
        PositionEncoding::UTF32 => line_index.source_location(offset, text),
    };

    #[allow(clippy::cast_possible_truncation)]
    Position {
        line: location.row.to_zero_indexed() as u32,
        character: location.column.to_zero_indexed() as u32,
    }
}

pub(crate) fn text_diff_to_edits<'a>(
    diff: &TextDiff<'a, 'a, 'a, str>,
    document: &Document,
    encoding: PositionEncoding,
) -> Vec<TextEdit> {
    let mut offset = TextSize::new(0);
    let mut edits = Vec::new();

    let mut changes = diff.iter_all_changes().peekable();

    while let Some(change) = changes.next() {
        match change.tag() {
            ChangeTag::Equal => {
                offset += change.value().text_len();
            }
            ChangeTag::Delete => {
                let start =
                    offset_to_position(offset, document.text(), document.line_index(), encoding);

                let mut end_offset = offset + change.value().text_len();
                let mut new_text = String::new();

                // Merge subsequent insertions and deletions into a single text edit.
                while changes.peek().is_some_and(|change| {
                    matches!(change.tag(), ChangeTag::Insert | ChangeTag::Delete)
                }) {
                    let change = changes.next().unwrap();
                    match change.tag() {
                        ChangeTag::Delete => {
                            end_offset += change.value().text_len();
                        }
                        ChangeTag::Insert => {
                            new_text.push_str(change.value());
                        }
                        ChangeTag::Equal => unreachable!(),
                    }
                }

                let end = offset_to_position(
                    end_offset,
                    document.text(),
                    document.line_index(),
                    encoding,
                );

                edits.push(TextEdit {
                    range: Range { start, end },
                    new_text,
                });

                offset = end_offset;
            }
            ChangeTag::Insert => {
                let position =
                    offset_to_position(offset, document.text(), document.line_index(), encoding);
                edits.push(TextEdit {
                    range: Range {
                        start: position,
                        end: position,
                    },
                    new_text: change.to_string(),
                });
            }
        }
    }

    // Reverse the edits because the edits are relative to each other
    edits.reverse();

    edits
}

pub(crate) fn text_range_to_range(
    text_range: TextRange,
    document: &Document,
    encoding: PositionEncoding,
) -> Range {
    Range {
        start: offset_to_position(
            text_range.start(),
            document.text(),
            document.line_index(),
            encoding,
        ),
        end: offset_to_position(
            text_range.end(),
            document.text(),
            document.line_index(),
            encoding,
        ),
    }
}

#[repr(u8)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
/// UTF 16 is the encoding supported by all LSP clients.
pub(crate) enum PositionEncoding {
    #[default]
    UTF16,

    /// Ruff's preferred encoding
    UTF8,

    /// Second choice because UTF32 uses a fixed 4 byte encoding for each character (makes conversion relatively easy)
    UTF32,
}

impl From<PositionEncoding> for PositionEncodingKind {
    fn from(value: PositionEncoding) -> Self {
        match value {
            PositionEncoding::UTF8 => PositionEncodingKind::UTF8,
            PositionEncoding::UTF16 => PositionEncodingKind::UTF16,
            PositionEncoding::UTF32 => PositionEncodingKind::UTF32,
        }
    }
}

pub(crate) struct AtomicPositionEncoding(AtomicU8);

impl AtomicPositionEncoding {
    pub(crate) const fn new(encoding: PositionEncoding) -> Self {
        Self(AtomicU8::new(encoding as u8))
    }

    pub(crate) fn get(&self) -> PositionEncoding {
        let encoding = self.0.load(Ordering::Relaxed);

        // SAFETY: SAFE because this wrapper guarantees that the atomic value only ever holds a valud
        // `PositionEncoding`.
        #[allow(unsafe_code)]
        unsafe {
            std::mem::transmute(encoding)
        }
    }

    pub(crate) fn set(&self, encoding: PositionEncoding) {
        self.0.store(encoding as u8, Ordering::Relaxed);
    }
}

impl PartialEq<PositionEncoding> for AtomicPositionEncoding {
    fn eq(&self, other: &PositionEncoding) -> bool {
        &self.get() == other
    }
}

impl PartialEq<AtomicPositionEncoding> for PositionEncoding {
    fn eq(&self, other: &AtomicPositionEncoding) -> bool {
        self == &other.get()
    }
}

impl From<PositionEncoding> for AtomicPositionEncoding {
    fn from(value: PositionEncoding) -> Self {
        AtomicPositionEncoding::new(value)
    }
}

impl From<AtomicPositionEncoding> for PositionEncoding {
    fn from(value: AtomicPositionEncoding) -> Self {
        value.get()
    }
}

impl Default for AtomicPositionEncoding {
    fn default() -> Self {
        Self::new(PositionEncoding::default())
    }
}

impl Debug for AtomicPositionEncoding {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AtomicPositionEncoding")
            .field(&self.get())
            .finish()
    }
}
