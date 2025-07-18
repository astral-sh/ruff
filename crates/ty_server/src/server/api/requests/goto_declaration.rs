use std::borrow::Cow;

use lsp_types::request::{GotoDeclaration, GotoDeclarationParams};
use lsp_types::{GotoDefinitionResponse, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::goto_declaration;
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct GotoDeclarationRequestHandler;

impl RequestHandler for GotoDeclarationRequestHandler {
    type RequestType = GotoDeclaration;
}

impl BackgroundDocumentRequestHandler for GotoDeclarationRequestHandler {
    fn document_url(params: &GotoDeclarationParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: GotoDeclarationParams,
    ) -> crate::server::Result<Option<GotoDefinitionResponse>> {
        if snapshot.client_settings().is_language_services_disabled() {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);
        let offset = params.text_document_position_params.position.to_text_size(
            &source,
            &line_index,
            snapshot.encoding(),
        );

        let Some(ranged) = goto_declaration(db, file, offset) else {
            return Ok(None);
        };

        if snapshot
            .resolved_client_capabilities()
            .supports_declaration_link()
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

impl RetriableRequestHandler for GotoDeclarationRequestHandler {}
