use std::borrow::Cow;

use lsp_types::{request::GotoDefinition, GotoDefinitionParams, GotoDefinitionResponse};

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};

pub(crate) struct GotoDefinitionHandler;

impl RequestHandler for GotoDefinitionHandler {
    type RequestType = GotoDefinition;
}

impl BackgroundDocumentRequestHandler for GotoDefinitionHandler {
    fn document_url(params: &GotoDefinitionParams) -> std::borrow::Cow<lsp_types::Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        _snapshot: crate::DocumentSnapshot,
        _db: red_knot_workspace::db::RootDatabase,
        _notifier: crate::server::client::Notifier,
        _params: GotoDefinitionParams,
    ) -> crate::server::api::Result<Option<GotoDefinitionResponse>> {
        log_err_msg!("WOULD HAVE TRIED LOOKUP");
        Ok(None)
    }
}
