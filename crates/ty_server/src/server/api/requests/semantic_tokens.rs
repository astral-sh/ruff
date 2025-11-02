use std::borrow::Cow;

use lsp_types::{SemanticTokens, SemanticTokensParams, SemanticTokensResult, Url};
use ruff_db::source::source_text;
use ty_project::ProjectDatabase;

use crate::db::Db;
use crate::server::api::semantic_tokens::generate_semantic_tokens;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct SemanticTokensRequestHandler;

impl RequestHandler for SemanticTokensRequestHandler {
    type RequestType = lsp_types::request::SemanticTokensFullRequest;
}

impl BackgroundDocumentRequestHandler for SemanticTokensRequestHandler {
    fn document_url(params: &SemanticTokensParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        _params: SemanticTokensParams,
    ) -> crate::server::Result<Option<SemanticTokensResult>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let mut range = None;

        if snapshot.document().is_cell() {
            if let Some(notebook_document) = db.notebook_document(file)
                && let Some(notebook) = source_text(db, file).as_notebook()
            {
                let cell_index = notebook_document.cell_index_by_uri(snapshot.url());

                range = cell_index.and_then(|index| notebook.cell_range(index));
            }
        }

        let lsp_tokens = generate_semantic_tokens(
            db,
            file,
            range,
            snapshot.encoding(),
            snapshot
                .resolved_client_capabilities()
                .supports_multiline_semantic_tokens(),
        );

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}

impl RetriableRequestHandler for SemanticTokensRequestHandler {}
