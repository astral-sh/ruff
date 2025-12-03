use std::borrow::Cow;

use crate::document::{FileRangeExt, PositionExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::request::HoverRequest;
use lsp_types::{HoverContents, HoverParams, MarkupContent, Url};
use ty_ide::{MarkupKind, hover};
use ty_project::ProjectDatabase;

pub(crate) struct HoverRequestHandler;

impl RequestHandler for HoverRequestHandler {
    type RequestType = HoverRequest;
}

impl BackgroundDocumentRequestHandler for HoverRequestHandler {
    fn document_url(params: &HoverParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: HoverParams,
    ) -> crate::server::Result<Option<lsp_types::Hover>> {
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

        let Some(range_info) = hover(db, file, offset) else {
            return Ok(None);
        };

        let (markup_kind, lsp_markup_kind) = if snapshot
            .resolved_client_capabilities()
            .prefers_markdown_in_hover()
        {
            (MarkupKind::Markdown, lsp_types::MarkupKind::Markdown)
        } else {
            (MarkupKind::PlainText, lsp_types::MarkupKind::PlainText)
        };

        let contents = range_info.display(db, markup_kind).to_string();

        Ok(Some(lsp_types::Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: lsp_markup_kind,
                value: contents,
            }),
            range: range_info
                .file_range()
                .to_lsp_range(db, snapshot.encoding())
                .map(|lsp_range| lsp_range.local_range()),
        }))
    }
}

impl RetriableRequestHandler for HoverRequestHandler {}
