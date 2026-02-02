use std::borrow::Cow;

use lsp_types::request::GotoDefinition;
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Url};
use ty_ide::goto_definition;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct GotoDefinitionRequestHandler;

impl RequestHandler for GotoDefinitionRequestHandler {
    type RequestType = GotoDefinition;
}

impl BackgroundDocumentRequestHandler for GotoDefinitionRequestHandler {
    fn document_url(params: &GotoDefinitionParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: GotoDefinitionParams,
    ) -> crate::server::Result<Option<GotoDefinitionResponse>> {
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

        let Some(ranged) = goto_definition(db, file, offset) else {
            return Ok(None);
        };

        if snapshot
            .resolved_client_capabilities()
            .supports_definition_link()
        {
            let src = Some(ranged.range);
            let links: Vec<_> = ranged
                .into_iter()
                .filter_map(|target| target.to_link(db, src, snapshot.encoding()))
                .collect();

            Ok(Some(GotoDefinitionResponse::Link(links)))
        } else {
            let locations: Vec<_> = ranged
                .into_iter()
                .filter_map(|target| target.to_location(db, snapshot.encoding()))
                .collect();

            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }
}

impl RetriableRequestHandler for GotoDefinitionRequestHandler {}
