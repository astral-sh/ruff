use std::borrow::Cow;

use crate::document::{RangeExt, TextSizeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::request::InlayHintRequest;
use lsp_types::{InlayHintParams, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::{InlayHintKind, InlayHintLabel, inlay_hints};
use ty_project::ProjectDatabase;

pub(crate) struct InlayHintRequestHandler;

impl RequestHandler for InlayHintRequestHandler {
    type RequestType = InlayHintRequest;
}

impl BackgroundDocumentRequestHandler for InlayHintRequestHandler {
    fn document_url(params: &InlayHintParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: InlayHintParams,
    ) -> crate::server::Result<Option<Vec<lsp_types::InlayHint>>> {
        let workspace_settings = snapshot.workspace_settings();
        if workspace_settings.is_language_services_disabled()
            || !workspace_settings.inlay_hints().any_enabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            return Ok(None);
        };

        let index = line_index(db, file);
        let source = source_text(db, file);

        let range = params
            .range
            .to_text_range(&source, &index, snapshot.encoding());

        let inlay_hints = inlay_hints(db, file, range, workspace_settings.inlay_hints());

        let inlay_hints = inlay_hints
            .into_iter()
            .map(|hint| lsp_types::InlayHint {
                position: hint
                    .position
                    .to_position(&source, &index, snapshot.encoding()),
                label: inlay_hint_label(&hint.label),
                kind: Some(inlay_hint_kind(&hint.kind)),
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

impl RetriableRequestHandler for InlayHintRequestHandler {}

fn inlay_hint_kind(inlay_hint_kind: &InlayHintKind) -> lsp_types::InlayHintKind {
    match inlay_hint_kind {
        InlayHintKind::Type => lsp_types::InlayHintKind::TYPE,
        InlayHintKind::CallArgumentName => lsp_types::InlayHintKind::PARAMETER,
    }
}

fn inlay_hint_label(inlay_hint_label: &InlayHintLabel) -> lsp_types::InlayHintLabel {
    let mut label_parts = Vec::new();
    for part in inlay_hint_label.parts() {
        label_parts.push(lsp_types::InlayHintLabelPart {
            value: part.text().into(),
            location: None,
            tooltip: None,
            command: None,
        });
    }
    lsp_types::InlayHintLabel::LabelParts(label_parts)
}
