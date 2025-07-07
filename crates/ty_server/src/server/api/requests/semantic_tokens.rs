use std::borrow::Cow;

use crate::DocumentSnapshot;
use crate::server::api::semantic_tokens::generate_semantic_tokens;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::client::Client;
use lsp_types::{SemanticTokens, SemanticTokensParams, SemanticTokensResult, Url};
use ty_project::ProjectDatabase;

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

        let lsp_tokens = generate_semantic_tokens(
            db, 
            file, 
            None, 
            snapshot.encoding(),
            snapshot.resolved_client_capabilities().semantic_tokens_multiline_support,
        );

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}

impl RetriableRequestHandler for SemanticTokensRequestHandler {}
