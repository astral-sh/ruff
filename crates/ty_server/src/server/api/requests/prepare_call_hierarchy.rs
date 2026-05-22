use std::borrow::Cow;

use lsp_types::request::CallHierarchyPrepare;
use lsp_types::{CallHierarchyItem, CallHierarchyPrepareParams, Url};
use ty_project::ProjectDatabase;

use crate::document::PositionExt;
use crate::server::api::call_hierarchy::convert_to_lsp_item;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

/// Handles `textDocument/prepareCallHierarchy`.
///
/// The initial step of an LSP call-hierarchy session: given a cursor position,
/// identify the function/method/class at that position and return one or more
/// `CallHierarchyItem`s. The client then sends those items back via
/// `callHierarchy/incomingCalls` / `callHierarchy/outgoingCalls`.
pub(crate) struct PrepareCallHierarchyRequestHandler;

impl RequestHandler for PrepareCallHierarchyRequestHandler {
    type RequestType = CallHierarchyPrepare;
}

impl BackgroundDocumentRequestHandler for PrepareCallHierarchyRequestHandler {
    fn document_url(params: &CallHierarchyPrepareParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: CallHierarchyPrepareParams,
    ) -> crate::server::Result<Option<Vec<CallHierarchyItem>>> {
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

        let Some(items) = ty_ide::prepare_call_hierarchy(db, file, offset) else {
            return Ok(None);
        };

        let lsp_items: Vec<_> = items
            .into_iter()
            .filter_map(|item| convert_to_lsp_item(db, item, snapshot.encoding()))
            .collect();

        if lsp_items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lsp_items))
        }
    }
}

impl RetriableRequestHandler for PrepareCallHierarchyRequestHandler {}
