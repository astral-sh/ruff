use std::borrow::Cow;

use crate::DocumentSnapshot;
use crate::document::PositionExt;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::session::client::Client;
use lsp_types::{
    SemanticToken, SemanticTokens, SemanticTokensParams, SemanticTokensRangeParams,
    SemanticTokensRangeResult, SemanticTokensResult, Url,
};
use ruff_db::source::{line_index, source_text};
use ruff_text_size::{TextLen, TextRange};
use ty_ide::semantic_tokens;
use ty_project::ProjectDatabase;

/// Common logic for generating semantic tokens, either for full document or a specific range.
/// If no range is provided, the entire file is processed.
fn generate_semantic_tokens(
    db: &ProjectDatabase,
    file: ruff_db::files::File,
    range: Option<TextRange>,
) -> Option<Vec<SemanticToken>> {
    let source = source_text(db, file);
    let line_index = line_index(db, file);

    let requested_range = range.unwrap_or_else(|| TextRange::new(0.into(), source.text_len()));
    let semantic_token_data = semantic_tokens(db, file, requested_range);

    let semantic_token_data = semantic_token_data?;

    // Convert semantic tokens to LSP format with delta encoding
    // Sort tokens by position to ensure proper delta encoding
    // This prevents integer underflow when computing deltas for out-of-order tokens
    let mut sorted_tokens = semantic_token_data.tokens;
    sorted_tokens.sort_by_key(|token| token.range.start());

    // Convert semantic tokens to LSP format
    let mut lsp_tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in sorted_tokens {
        let start_position = line_index.line_column(token.range.start(), &source);
        let line = u32::try_from(start_position.line.to_zero_indexed()).unwrap_or(u32::MAX);
        let character = u32::try_from(start_position.column.to_zero_indexed()).unwrap_or(u32::MAX);
        let length = token.range.len().to_u32();
        let token_type = token.token_type as u32;
        let token_modifiers = token
            .modifiers
            .iter()
            .fold(0u32, |acc, modifier| acc | (1 << (*modifier as u32)));

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

    Some(lsp_tokens)
}

pub(crate) struct SemanticTokensRequestHandler;

impl RequestHandler for SemanticTokensRequestHandler {
    type RequestType = lsp_types::request::SemanticTokensFullRequest;
}

impl BackgroundDocumentRequestHandler for SemanticTokensRequestHandler {
    fn document_url(params: &SemanticTokensParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: SemanticTokensParams,
    ) -> crate::server::Result<Option<SemanticTokensResult>> {
        if snapshot.client_settings().is_language_services_disabled() {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let Some(lsp_tokens) = generate_semantic_tokens(db, file, None) else {
            return Ok(None);
        };

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}

pub(crate) struct SemanticTokensRangeRequestHandler;

impl RequestHandler for SemanticTokensRangeRequestHandler {
    type RequestType = lsp_types::request::SemanticTokensRangeRequest;
}

impl BackgroundDocumentRequestHandler for SemanticTokensRangeRequestHandler {
    fn document_url(params: &SemanticTokensRangeParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: SemanticTokensRangeParams,
    ) -> crate::server::Result<Option<SemanticTokensRangeResult>> {
        if snapshot.client_settings().is_language_services_disabled() {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);

        // Convert LSP range to text offsets
        let start_offset =
            params
                .range
                .start
                .to_text_size(&source, &line_index, snapshot.encoding());

        let end_offset = params
            .range
            .end
            .to_text_size(&source, &line_index, snapshot.encoding());

        let requested_range = ruff_text_size::TextRange::new(start_offset, end_offset);

        let Some(lsp_tokens) = generate_semantic_tokens(db, file, Some(requested_range)) else {
            return Ok(None);
        };

        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}
