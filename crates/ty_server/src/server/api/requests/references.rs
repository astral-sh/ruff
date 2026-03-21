use std::borrow::Cow;

use lsp_types::request::References;
use lsp_types::{Location, ReferenceParams, Url};
use ty_ide::find_references;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct ReferencesRequestHandler;

impl RequestHandler for ReferencesRequestHandler {
    type RequestType = References;
}

impl BackgroundDocumentRequestHandler for ReferencesRequestHandler {
    fn document_url(params: &ReferenceParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: ReferenceParams,
    ) -> crate::server::Result<Option<Vec<Location>>> {
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

        let include_declaration = params.context.include_declaration;

        let Some(references_result) = find_references(db, file, offset, include_declaration) else {
            return Ok(None);
        };

        let locations: Vec<_> = references_result
            .into_iter()
            .filter_map(|target| target.to_location(db, snapshot.encoding()))
            .collect();

        Ok(Some(locations))
    }
}

impl RetriableRequestHandler for ReferencesRequestHandler {}
