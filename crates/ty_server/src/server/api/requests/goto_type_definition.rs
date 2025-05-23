use std::borrow::Cow;

use lsp_types::request::{GotoTypeDefinition, GotoTypeDefinitionParams};
use lsp_types::{GotoDefinitionResponse, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::goto_type_definition;
use ty_project::ProjectDatabase;

use crate::DocumentSnapshot;
use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::client::Notifier;

pub(crate) struct GotoTypeDefinitionRequestHandler;

impl RequestHandler for GotoTypeDefinitionRequestHandler {
    type RequestType = GotoTypeDefinition;
}

impl BackgroundDocumentRequestHandler for GotoTypeDefinitionRequestHandler {
    fn document_url(params: &GotoTypeDefinitionParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: GotoTypeDefinitionParams,
    ) -> crate::server::Result<Option<GotoDefinitionResponse>> {
        let Some(file) = snapshot.file(db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);
        let offset = params.text_document_position_params.position.to_text_size(
            &source,
            &line_index,
            snapshot.encoding(),
        );

        let Some(ranged) = goto_type_definition(db, file, offset) else {
            return Ok(None);
        };

        if snapshot
            .resolved_client_capabilities()
            .type_definition_link_support
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
