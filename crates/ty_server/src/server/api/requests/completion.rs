use std::borrow::Cow;

use lsp_types::request::Completion;
use lsp_types::{CompletionItem, CompletionParams, CompletionResponse, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::completion;
use ty_project::ProjectDatabase;

use crate::DocumentSnapshot;
use crate::document::PositionExt;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::client::Notifier;

pub(crate) struct CompletionRequestHandler;

impl RequestHandler for CompletionRequestHandler {
    type RequestType = Completion;
}

impl BackgroundDocumentRequestHandler for CompletionRequestHandler {
    fn document_url(params: &CompletionParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: CompletionParams,
    ) -> crate::server::Result<Option<CompletionResponse>> {
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

        let items: Vec<CompletionItem> = completions
            .into_iter()
            .map(|comp| CompletionItem {
                label: comp.label,
                ..Default::default()
            })
            .collect();
        let response = CompletionResponse::Array(items);
        Ok(Some(response))
    }
}
