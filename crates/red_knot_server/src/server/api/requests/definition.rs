use std::borrow::Cow;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use lsp_types::{request::GotoDefinition, GotoDefinitionParams, GotoDefinitionResponse, Location};
// XXX the one place where I'm using something from red_knot_python_semantic
// maybe need to just move the type?
use red_knot_python_semantic::location::DefLocation;
use red_knot_workspace::db::RootDatabase;

pub(crate) struct GotoDefinitionHandler;

impl RequestHandler for GotoDefinitionHandler {
    type RequestType = GotoDefinition;
}

impl BackgroundDocumentRequestHandler for GotoDefinitionHandler {
    fn document_url(params: &GotoDefinitionParams) -> std::borrow::Cow<lsp_types::Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: crate::DocumentSnapshot,
        db: RootDatabase,
        _notifier: crate::server::client::Notifier,
        params: GotoDefinitionParams,
    ) -> crate::server::api::Result<Option<GotoDefinitionResponse>> {
        let Some(file) = snapshot.file(&db) else {
            // XXX not sure if this should be considered an error or not...
            return Ok(None);
        };

        let lookup_result =
            db.definition_at_location(file, params.text_document_position_params.position);
        match lookup_result {
            Some(DefLocation::Location { url, range }) => {
                let result = Location { uri: url, range };
                return Ok(Some(GotoDefinitionResponse::Array(vec![result])));
            }
            Some(DefLocation::Todo { s }) => {
                log_err_msg!("GOT TODO: {}", s);
            }
            None => {
                log_err_msg!("NOTHING FOUND");
            }
        }
        Ok(None)
    }
}
