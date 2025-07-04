use std::borrow::Cow;

use crate::DocumentSnapshot;
use crate::document::PositionExt;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::api::semantic_tokens::generate_semantic_tokens;
use crate::session::client::Client;
use lsp_types::{
    SemanticTokens, SemanticTokensRangeParams, SemanticTokensRangeResult, Url,
};
use ruff_db::source::{line_index, source_text};
use ty_project::ProjectDatabase;

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

        let lsp_tokens = generate_semantic_tokens(db, file, Some(requested_range));

        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}
