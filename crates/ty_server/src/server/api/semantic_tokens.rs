use lsp_types::SemanticToken;
use ruff_db::source::{line_index, source_text};
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
    let source = source_text(db, file);

    let mut tokens =
        ty_ide::encoded_semantic_tokens(db, file, range, encoding.into(), multiline_token_support)
            .into_iter()
            .map(convert_semantic_token)
            .collect::<Vec<_>>();

    if source.as_notebook().is_some() {
        if let (Some(range), Some(first)) = (range, tokens.first_mut()) {
            let line_index = line_index(db, file);
            let cell_start_global_line = u32::try_from(
                line_index
                    .source_location(range.start(), source.as_str(), encoding.into())
                    .line
                    .to_zero_indexed(),
            )
            .unwrap_or(0);

            // [Note]:
            // 1. `ty_server` constrains `range` to the current cell's global offset at the request level.
            // 2. Notebook responses must use cell-local coordinates.
            // 3. `SemanticToken` uses delta encoding, so only the first token's `delta_line` (relative to line 0)
            //    needs adjustment. All subsequent relative deltas remain correct because every token shifts by the same offset.
            first.delta_line -= cell_start_global_line;
        }
    }

    tokens
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
