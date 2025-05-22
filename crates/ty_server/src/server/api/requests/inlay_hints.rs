use std::borrow::Cow;

use crate::DocumentSnapshot;
use crate::document::{RangeExt, TextSizeExt};
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::client::Notifier;
use lsp_types::request::InlayHintRequest;
use lsp_types::{InlayHintParams, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::inlay_hints;
use ty_project::ProjectDatabase;

pub(crate) struct InlayHintRequestHandler;

impl RequestHandler for InlayHintRequestHandler {
    type RequestType = InlayHintRequest;
}

impl BackgroundDocumentRequestHandler for InlayHintRequestHandler {
    fn document_url(params: &InlayHintParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: InlayHintParams,
    ) -> crate::server::Result<Option<Vec<lsp_types::InlayHint>>> {
        let Some(file) = snapshot.file(db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let index = line_index(db, file);
        let source = source_text(db, file);

        let range = params
            .range
            .to_text_range(&source, &index, snapshot.encoding());

        let inlay_hints = inlay_hints(db, file, range);

        let inlay_hints = inlay_hints
            .into_iter()
            .map(|hint| lsp_types::InlayHint {
                position: hint
                    .position
                    .to_position(&source, &index, snapshot.encoding()),
                label: lsp_types::InlayHintLabel::String(hint.display(db).to_string()),
                kind: Some(lsp_types::InlayHintKind::TYPE),
                tooltip: None,
                padding_left: None,
                padding_right: None,
                data: None,
                text_edits: None,
            })
            .collect();

        Ok(Some(inlay_hints))
    }
}
