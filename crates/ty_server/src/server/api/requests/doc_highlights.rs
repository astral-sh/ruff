use std::borrow::Cow;

use lsp_types::request::DocumentHighlightRequest;
use lsp_types::{DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams, Url};
use ty_ide::{ReferenceKind, document_highlights};
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToRangeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct DocumentHighlightRequestHandler;

impl RequestHandler for DocumentHighlightRequestHandler {
    type RequestType = DocumentHighlightRequest;
}

impl BackgroundDocumentRequestHandler for DocumentHighlightRequestHandler {
    fn document_url(params: &DocumentHighlightParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: DocumentHighlightParams,
    ) -> crate::server::Result<Option<Vec<DocumentHighlight>>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.url(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let Some(highlights_result) = document_highlights(db, file, offset) else {
            return Ok(None);
        };

        let highlights: Vec<_> = highlights_result
            .into_iter()
            .filter_map(|target| {
                let range = target
                    .range()
                    .to_lsp_range(db, file, snapshot.encoding())?
                    .local_range();

                let kind = match target.kind() {
                    ReferenceKind::Read => Some(DocumentHighlightKind::READ),
                    ReferenceKind::Write => Some(DocumentHighlightKind::WRITE),
                    ReferenceKind::Other => Some(DocumentHighlightKind::TEXT),
                };

                Some(DocumentHighlight { range, kind })
            })
            .collect();

        Ok(Some(highlights))
    }
}

impl RetriableRequestHandler for DocumentHighlightRequestHandler {}
