use std::borrow::Cow;

use lsp_types::request::Completion;
use lsp_types::{CompletionItem, CompletionParams, CompletionResponse, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::completion;
use ty_project::ProjectDatabase;

use crate::DocumentSnapshot;
use crate::document::PositionExt;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::session::client::Client;

pub(crate) struct CompletionRequestHandler;

impl RequestHandler for CompletionRequestHandler {
    type RequestType = Completion;
}

impl BackgroundDocumentRequestHandler for CompletionRequestHandler {
    const RETRY_ON_CANCELLATION: bool = true;

    fn document_url(params: &CompletionParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: CompletionParams,
    ) -> crate::server::Result<Option<CompletionResponse>> {
        if snapshot.client_settings().is_language_services_disabled() {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);
        let offset = params.text_document_position.position.to_text_size(
            &source,
            &line_index,
            snapshot.encoding(),
        );
        let completions = completion(db, file, offset);
        if completions.is_empty() {
            return Ok(None);
        }

        let max_index_len = completions.len().saturating_sub(1).to_string().len();
        let items: Vec<CompletionItem> = completions
            .into_iter()
            .enumerate()
            .map(|(i, comp)| CompletionItem {
                label: comp.label,
                sort_text: Some(format!("{i:-max_index_len$}")),
                ..Default::default()
            })
            .collect();
        let response = CompletionResponse::Array(items);
        Ok(Some(response))
    }
}
