use std::borrow::Cow;

use lsp_types::{SemanticTokens, SemanticTokensParams, Uri};
use ty_project::ProjectDatabase;

use crate::document::cell_range;
use crate::server::api::semantic_tokens::generate_semantic_tokens;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct SemanticTokensRequestHandler;

impl RequestHandler for SemanticTokensRequestHandler {
    type RequestType = lsp_types::SemanticTokensRequest;
}

impl BackgroundDocumentRequestHandler for SemanticTokensRequestHandler {
    fn document_uri(params: &SemanticTokensParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        _params: SemanticTokensParams,
    ) -> crate::server::Result<Option<SemanticTokens>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.document().to_notebook_or_file(db) else {
            return Ok(None);
        };

        // If this document is a notebook cell, limit the highlighting range
        // to the lines of this cell (instead of highlighting the entire notebook).
        // Not only avoids this unnecessary work, this is also required
        // because all ranges in the response must be within this **this document**.
        //
        // A cell without a range in the notebook's Python source, such as one that uses a cell
        // magic, has nothing to highlight.
        let cell_range = if snapshot.document().is_cell() {
            let Some(range) = cell_range(db, file, snapshot.document().uri()) else {
                return Ok(None);
            };
            Some(range)
        } else {
            None
        };

        let lsp_tokens = generate_semantic_tokens(
            db,
            file,
            cell_range,
            snapshot.encoding(),
            snapshot
                .resolved_client_capabilities()
                .supports_multiline_semantic_tokens(),
        );

        Ok(Some(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        }))
    }
}

impl RetriableRequestHandler for SemanticTokensRequestHandler {}
