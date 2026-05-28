use std::borrow::Cow;

use lsp_types::request::OnTypeFormatting;
use lsp_types::{DocumentOnTypeFormattingParams, TextEdit, Url};
use ruff_text_size::Ranged;
use ty_ide::on_type_formatting;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToRangeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct OnTypeFormattingRequestHandler;

impl RequestHandler for OnTypeFormattingRequestHandler {
    type RequestType = OnTypeFormatting;
}

impl BackgroundDocumentRequestHandler for OnTypeFormattingRequestHandler {
    fn document_url(params: &DocumentOnTypeFormattingParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: DocumentOnTypeFormattingParams,
    ) -> crate::server::Result<Option<Vec<TextEdit>>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position.position.to_text_size(
            db,
            file,
            snapshot.url(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let Some(edit) = on_type_formatting(db, file, offset, &params.ch) else {
            return Ok(None);
        };

        let Some(range) = edit
            .range()
            .to_lsp_range(db, file, snapshot.encoding())
            .map(|range| range.local_range())
        else {
            return Ok(None);
        };

        Ok(Some(vec![TextEdit {
            range,
            new_text: edit.content().unwrap_or_default().to_string(),
        }]))
    }
}

impl RetriableRequestHandler for OnTypeFormattingRequestHandler {}
