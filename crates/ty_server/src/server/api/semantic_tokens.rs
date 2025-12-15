use lsp_types::SemanticToken;
use ruff_db::source::source_text;
use ruff_text_size::{Ranged, TextRange};
use ty_ide::semantic_tokens;
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
    let semantic_token_data = semantic_tokens(db, file, range);

    // Convert semantic tokens to LSP format
    let mut lsp_tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in &*semantic_token_data {
        let Some(lsp_range) = token
            .range()
            .to_lsp_range(db, file, encoding)
            .map(|lsp_range| lsp_range.local_range())
        else {
            continue;
        };

        let line = lsp_range.start.line;
        let character = lsp_range.start.character;

        // Calculate length in the negotiated encoding
        let length = if !multiline_token_support && lsp_range.start.line != lsp_range.end.line {
            // Token spans multiple lines but client doesn't support it
            // Clamp to the end of the current line
            if let Some(line_text) = source.lines().nth(lsp_range.start.line as usize) {
                let line_length_in_encoding = match encoding {
                    PositionEncoding::UTF8 => line_text.len().try_into().unwrap_or(u32::MAX),
                    PositionEncoding::UTF16 => line_text
                        .encode_utf16()
                        .count()
                        .try_into()
                        .unwrap_or(u32::MAX),
                    PositionEncoding::UTF32 => {
                        line_text.chars().count().try_into().unwrap_or(u32::MAX)
                    }
                };
                line_length_in_encoding.saturating_sub(lsp_range.start.character)
            } else {
                0
            }
        } else {
            // Either client supports multiline tokens or this is a single-line token
            // Use the difference between start and end character positions
            if lsp_range.start.line == lsp_range.end.line {
                lsp_range.end.character - lsp_range.start.character
            } else {
                // Multiline token and client supports it - calculate full token length
                let token_text = &source[token.range()];
                match encoding {
                    PositionEncoding::UTF8 => token_text.len().try_into().unwrap_or(u32::MAX),
                    PositionEncoding::UTF16 => token_text
                        .encode_utf16()
                        .count()
                        .try_into()
                        .unwrap_or(u32::MAX),
                    PositionEncoding::UTF32 => {
                        token_text.chars().count().try_into().unwrap_or(u32::MAX)
                    }
                }
            }
        };
        let token_type = token.token_type as u32;
        let token_modifiers = token.modifiers.bits();

        // LSP semantic tokens are encoded as deltas
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            character - prev_start
        } else {
            character
        };

        lsp_tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: token_modifiers,
        });

        prev_line = line;
        prev_start = character;
    }

    lsp_tokens
}
