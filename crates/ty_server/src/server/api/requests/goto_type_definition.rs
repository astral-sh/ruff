use std::borrow::Cow;

use lsp_types::{TypeDefinitionParams, TypeDefinitionRequest};
use lsp_types::{TypeDefinitionResponse, Uri};
use ty_ide::goto_type_definition;
use ty_project::{ProjectDatabase, SemanticDb as _};

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct GotoTypeDefinitionRequestHandler;

impl RequestHandler for GotoTypeDefinitionRequestHandler {
    type RequestType = TypeDefinitionRequest;
}

impl BackgroundDocumentRequestHandler for GotoTypeDefinitionRequestHandler {
    fn document_uri(params: &TypeDefinitionParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: TypeDefinitionParams,
    ) -> crate::server::Result<Option<TypeDefinitionResponse>> {
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

        let Some(ranged) = goto_type_definition(db, db.program_file(file), offset) else {
            return Ok(None);
        };

        if snapshot
            .resolved_client_capabilities()
            .supports_type_definition_link()
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

            Ok(Some(TypeDefinitionResponse::Definition(locations.into())))
        }
    }
}

impl RetriableRequestHandler for GotoTypeDefinitionRequestHandler {}
