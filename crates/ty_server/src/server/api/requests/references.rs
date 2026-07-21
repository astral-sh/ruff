use std::borrow::Cow;

use lsp_types::ReferencesRequest;
use lsp_types::{Location, ReferenceParams, Uri};
use ruff_db::PythonFile;
use ty_ide::find_references;
use ty_project::ProjectDatabase;
use ty_project::SemanticDb as _;

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct ReferencesRequestHandler;

impl RequestHandler for ReferencesRequestHandler {
    type RequestType = ReferencesRequest;
}

impl BackgroundDocumentRequestHandler for ReferencesRequestHandler {
    fn document_uri(params: &ReferenceParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
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

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.uri(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let include_declaration = params.context.include_declaration;

        let Some(references_result) = find_references(
            db,
            PythonFile::new(db, file, db.python_version()),
            offset,
            include_declaration,
        ) else {
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
