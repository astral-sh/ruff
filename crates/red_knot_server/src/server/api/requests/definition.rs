use std::borrow::Cow;

use lsp_types::{
    request::GotoDefinition, GotoDefinitionParams, GotoDefinitionResponse, Location, Range,
};

use crate::{
    server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler},
    session::DefLocation,
};

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
        db: red_knot_workspace::db::RootDatabase,
        _notifier: crate::server::client::Notifier,
        params: GotoDefinitionParams,
    ) -> crate::server::api::Result<Option<GotoDefinitionResponse>> {
        log_err_msg!("ATTEMPTING LOOKUP...");
        let lookup_result =
            snapshot.definition_at_location(params.text_document_position_params.position, &db);

        match lookup_result {
            Some(DefLocation::Location { url, range }) => {
                eprintln!("GOT SOMETHING!");
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
