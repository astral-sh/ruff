use std::borrow::Cow;

use lsp_types::{ImplementationParams, ImplementationRequest, ImplementationResponse, Uri};
use ty_ide::goto_implementation;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct GotoImplementationRequestHandler;

impl RequestHandler for GotoImplementationRequestHandler {
    type RequestType = ImplementationRequest;
}

impl BackgroundDocumentRequestHandler for GotoImplementationRequestHandler {
    fn document_uri(params: &ImplementationParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: ImplementationParams,
    ) -> crate::server::Result<Option<ImplementationResponse>> {
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

        let Some(ranged) = goto_implementation(db, file, offset) else {
            return Ok(None);
        };

        if snapshot
            .resolved_client_capabilities()
            .supports_implementation_link()
        {
            let src = Some(ranged.range);
            let links: Vec<_> = ranged
                .into_iter()
                .filter_map(|target| target.to_link(db, src, snapshot.encoding()))
                .collect();

            Ok(Some(links.into()))
        } else {
            let locations: Vec<_> = ranged
                .into_iter()
                .filter_map(|target| target.to_location(db, snapshot.encoding()))
                .collect();

            Ok(Some(ImplementationResponse::Definition(locations.into())))
        }
    }
}

impl RetriableRequestHandler for GotoImplementationRequestHandler {}
