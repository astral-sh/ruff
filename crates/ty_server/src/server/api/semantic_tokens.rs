use lsp_types::SemanticToken;
use ruff_text_size::TextRange;
use ty_project::ProjectDatabase;

use crate::document::PositionEncoding;

/// Common logic for generating semantic tokens, either for full document or a specific range.
/// If no range is provided, the entire file is processed.
pub(crate) fn generate_semantic_tokens(
    db: &ProjectDatabase,
    file: ruff_db::files::File,
    range: Option<TextRange>,
    encoding: PositionEncoding,
    multiline_token_support: bool,
) -> Vec<SemanticToken> {
    ty_ide::encoded_semantic_tokens(db, file, range, encoding.into(), multiline_token_support)
        .into_iter()
        .map(convert_semantic_token)
        .collect()
}

fn convert_semantic_token(token: ty_ide::EncodedSemanticToken) -> SemanticToken {
    SemanticToken {
        delta_line: token.delta_line,
        delta_start: token.delta_start,
        length: token.length,
        token_type: token.token_type,
        token_modifiers_bitset: token.token_modifiers_bitset,
    }
}
