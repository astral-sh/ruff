use lsp_types::SemanticToken;
use ruff_db::source::{line_index, source_text};
use ruff_source_file::OneIndexed;
use ruff_text_size::{Ranged, TextRange};
use ty_ide::{SemanticTokenModifier, SemanticTokenType, semantic_tokens};
use ty_project::ProjectDatabase;

use crate::document::{PositionEncoding, ToRangeExt};

/// Common logic for generating semantic tokens, either for full document or a specific range.
/// If no range is provided, the entire file is processed.
pub(crate) fn generate_semantic_tokens(
    db: &ProjectDatabase,
    file: ruff_db::files::File,
    range: Option<TextRange>,
    encoding: PositionEncoding,
    multiline_token_support: bool,
) -> Vec<SemanticToken> {
    let source = source_text(db, file);
    let line_index = line_index(db, file);
    let semantic_token_data = semantic_tokens(db, file, range);

    let mut encoder = Encoder {
        tokens: Vec::with_capacity(semantic_token_data.len()),
        prev_line: 0,
        prev_start: 0,
    };

    for token in &*semantic_token_data {
        let Some(lsp_range) = token
            .range()
            .to_lsp_range(db, file, encoding)
            .map(|lsp_range| lsp_range.local_range())
        else {
            continue;
        };

        if lsp_range.start.line == lsp_range.end.line {
            let len = lsp_range.end.character - lsp_range.start.character;
            encoder.push_token_at(lsp_range.start, len, token.token_type, token.modifiers);
        } else if multiline_token_support {
            // If the client supports multiline-tokens,
            // compute the length of the entire range.
            let mut len = 0;

            for line in lsp_range.start.line..lsp_range.end.line {
                let line_len = line_index.line_len(
                    OneIndexed::from_zero_indexed(line as usize),
                    &source,
                    encoding.into(),
                );

                len += u32::try_from(line_len).unwrap();
            }

            // Subtract the first line because we added the length from the beginning.
            len -= lsp_range.start.character;
            // We didn't compute the length of the last line, add it now.
            len += lsp_range.end.character;

            encoder.push_token_at(lsp_range.start, len, token.token_type, token.modifiers);
        } else {
            // Multiline token but the client only supports single line tokens
            // Push a token for each line.
            for line in lsp_range.start.line..=lsp_range.end.line {
                let start_character = if line == lsp_range.start.line {
                    lsp_range.start.character
                } else {
                    0
                };

                let start = lsp_types::Position {
                    line,
                    character: start_character,
                };

                let end = if line == lsp_range.end.line {
                    lsp_range.end.character
                } else {
                    let line_len = line_index.line_len(
                        OneIndexed::from_zero_indexed(line as usize),
                        &source,
                        encoding.into(),
                    );
                    u32::try_from(line_len).unwrap()
                };

                let len = end - start.character;

                encoder.push_token_at(start, len, token.token_type, token.modifiers);
            }
        }
    }

    encoder.tokens
}

struct Encoder {
    tokens: Vec<SemanticToken>,
    prev_line: u32,
    prev_start: u32,
}

impl Encoder {
    fn push_token_at(
        &mut self,
        start: lsp_types::Position,
        length: u32,
        ty: SemanticTokenType,
        modifiers: SemanticTokenModifier,
    ) {
        // LSP semantic tokens are encoded as deltas
        let delta_line = start.line - self.prev_line;
        let delta_start = if delta_line == 0 {
            start.character - self.prev_start
        } else {
            start.character
        };

        let token_type = ty as u32;
        let token_modifiers = modifiers.bits();

        self.tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: token_modifiers,
        });

        self.prev_line = start.line;
        self.prev_start = start.character;
    }
}
