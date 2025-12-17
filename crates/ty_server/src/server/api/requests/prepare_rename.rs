use std::borrow::Cow;

use lsp_types::request::PrepareRenameRequest;
use lsp_types::{PrepareRenameResponse, TextDocumentPositionParams, Url};
use ty_ide::can_rename;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToRangeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct PrepareRenameRequestHandler;

impl RequestHandler for PrepareRenameRequestHandler {
    type RequestType = PrepareRenameRequest;
}

impl BackgroundDocumentRequestHandler for PrepareRenameRequestHandler {
    fn document_url(params: &TextDocumentPositionParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: TextDocumentPositionParams,
    ) -> crate::server::Result<Option<PrepareRenameResponse>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) =
            params
                .position
                .to_text_size(db, file, snapshot.url(), snapshot.encoding())
        else {
            return Ok(None);
        };

        let Some(range) = can_rename(db, file, offset) else {
            return Ok(None);
        };

        let Some(lsp_range) = range
            .to_lsp_range(db, file, snapshot.encoding())
            .map(|lsp_range| lsp_range.local_range())
        else {
            return Ok(None);
        };

        Ok(Some(PrepareRenameResponse::Range(lsp_range)))
    }
}

impl RetriableRequestHandler for PrepareRenameRequestHandler {}
