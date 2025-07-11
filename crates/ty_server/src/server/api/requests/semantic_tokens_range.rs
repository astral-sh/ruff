use std::borrow::Cow;

use crate::document::RangeExt;
use crate::server::api::semantic_tokens::generate_semantic_tokens;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::{SemanticTokens, SemanticTokensRangeParams, SemanticTokensRangeResult, Url};
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

        let file = match snapshot.file(db) {
            Ok(file) => file,
            Err(err) => {
                tracing::debug!("Failed to resolve file for {:?}: {}", params, err);
                return Ok(None);
            }
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);

        // Convert LSP range to text offsets
        let requested_range = params
            .range
            .to_text_range(&source, &line_index, snapshot.encoding());

        let lsp_tokens = generate_semantic_tokens(
            db,
            file,
            Some(requested_range),
            snapshot.encoding(),
            snapshot
                .resolved_client_capabilities()
                .semantic_tokens_multiline_support,
        );

        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}

impl RetriableRequestHandler for SemanticTokensRangeRequestHandler {}
