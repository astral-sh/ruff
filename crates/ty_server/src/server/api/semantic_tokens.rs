use lsp_types::SemanticToken;
use ruff_db::source::{line_index, source_text};
use ruff_text_size::{TextLen, TextRange};
use ty_ide::semantic_tokens;
use ty_project::ProjectDatabase;

/// Common logic for generating semantic tokens, either for full document or a specific range.
/// If no range is provided, the entire file is processed.
pub(crate) fn generate_semantic_tokens(
    db: &ProjectDatabase,
    file: ruff_db::files::File,
    range: Option<TextRange>,
) -> Vec<SemanticToken> {
    let source = source_text(db, file);
    let line_index = line_index(db, file);

    let requested_range = range.unwrap_or_else(|| TextRange::new(0.into(), source.text_len()));
    let semantic_token_data = semantic_tokens(db, file, Some(requested_range));

    // Convert semantic tokens to LSP format
    let mut lsp_tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in &*semantic_token_data {
        let start_position = line_index.line_column(token.range.start(), &source);
        let line = u32::try_from(start_position.line.to_zero_indexed()).unwrap_or(u32::MAX);
        let character = u32::try_from(start_position.column.to_zero_indexed()).unwrap_or(u32::MAX);
        let length = token.range.len().to_u32();
        let token_type = token.token_type as u32;
        let token_modifiers = token
            .modifiers
            .to_lsp_indices()
            .into_iter()
            .fold(0u32, |acc, modifier_index| acc | (1 << modifier_index));

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
