use std::borrow::Cow;

use lsp_types::request::{GotoTypeDefinition, GotoTypeDefinitionParams};
use lsp_types::{GotoDefinitionResponse, Url};
use red_knot_ide::go_to_type_definition;
use red_knot_project::ProjectDatabase;
use ruff_db::source::{line_index, source_text};

use crate::document::{PositionExt, ToLink};
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::client::Notifier;
use crate::DocumentSnapshot;

pub(crate) struct GotoTypeDefinitionRequestHandler;

impl RequestHandler for GotoTypeDefinitionRequestHandler {
    type RequestType = GotoTypeDefinition;
}

impl BackgroundDocumentRequestHandler for GotoTypeDefinitionRequestHandler {
    fn document_url(params: &GotoTypeDefinitionParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: ProjectDatabase,
        _notifier: Notifier,
        params: GotoTypeDefinitionParams,
    ) -> crate::server::Result<Option<GotoDefinitionResponse>> {
        let Some(file) = snapshot.file(&db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let source = source_text(&db, file);
        let line_index = line_index(&db, file);
        let offset = params.text_document_position_params.position.to_text_size(
            &source,
            &line_index,
            snapshot.encoding(),
        );

        let Some(range_info) = go_to_type_definition(&db, file, offset) else {
            return Ok(None);
        };

        if snapshot
            .resolved_client_capabilities()
            .declaration_link_support
        {
            let links: Vec<_> = range_info
                .info
                .into_iter()
                .filter_map(|target| {
                    target.to_link(&db, Some(range_info.range), snapshot.encoding())
                })
                .collect();

            Ok(Some(GotoDefinitionResponse::Link(links)))
        } else {
            let locations: Vec<_> = range_info
                .info
                .into_iter()
                .filter_map(|target| target.to_location(&db, snapshot.encoding()))
                .collect();

            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }
}
