use std::borrow::Cow;

use lsp_types::{SemanticTokens, SemanticTokensRangeParams, SemanticTokensRangeResult, Url};
use ty_project::ProjectDatabase;

use crate::document::RangeExt;
use crate::server::api::semantic_tokens::generate_semantic_tokens;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct SemanticTokensRangeRequestHandler;

impl RequestHandler for SemanticTokensRangeRequestHandler {
    type RequestType = lsp_types::request::SemanticTokensRangeRequest;
}

impl BackgroundDocumentRequestHandler for SemanticTokensRangeRequestHandler {
    fn document_url(params: &SemanticTokensRangeParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: SemanticTokensRangeParams,
    ) -> crate::server::Result<Option<SemanticTokensRangeResult>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        // Convert LSP range to text offsets
        let Some(requested_range) =
            params
                .range
                .to_text_range(db, file, snapshot.url(), snapshot.encoding())
        else {
            return Ok(None);
        };

        let lsp_tokens = generate_semantic_tokens(
            db,
            file,
            Some(requested_range),
            snapshot.encoding(),
            snapshot
                .resolved_client_capabilities()
                .supports_multiline_semantic_tokens(),
        );

        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}

impl RetriableRequestHandler for SemanticTokensRangeRequestHandler {}
