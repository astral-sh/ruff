use std::borrow::Cow;

use lsp_types::CallHierarchyPrepareRequest;
use lsp_types::{CallHierarchyItem, CallHierarchyPrepareParams, Uri};
use ruff_db::PythonFile;
use ty_module_resolver::Db as _;
use ty_project::ProjectDatabase;

use crate::PositionEncoding;
use crate::document::{PositionExt, ToRangeExt as _};
use crate::server::api::symbols::convert_symbol_kind;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use crate::system::file_to_uri;

/// Handles `textDocument/prepareCallHierarchy`.
///
/// The initial step of an LSP call-hierarchy session: given a cursor position,
/// identify the function/method/class at that position and return one or more
/// `CallHierarchyItem`s. The client then sends those items back via
/// `callHierarchy/incomingCalls` / `callHierarchy/outgoingCalls`.
pub(crate) struct PrepareCallHierarchyRequestHandler;

impl RequestHandler for PrepareCallHierarchyRequestHandler {
    type RequestType = CallHierarchyPrepareRequest;
}

impl BackgroundDocumentRequestHandler for PrepareCallHierarchyRequestHandler {
    fn document_uri(params: &CallHierarchyPrepareParams) -> Cow<'_, Uri> {
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
            snapshot.uri(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let Some(items) = ty_ide::prepare_call_hierarchy(
            db,
            PythonFile::new(db, file, db.python_version()),
            offset,
        ) else {
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

pub(super) fn convert_to_lsp_item(
    db: &ProjectDatabase,
    item: ty_ide::CallHierarchyItem,
    encoding: PositionEncoding,
) -> Option<CallHierarchyItem> {
    let uri = file_to_uri(db, item.file)?;
    let full_range = item.full_range.to_lsp_range(db, item.file, encoding)?;
    let selection_range = item.selection_range.to_lsp_range(db, item.file, encoding)?;

    let kind = convert_symbol_kind(item.kind);

    Some(CallHierarchyItem {
        name: item.name.into(),
        kind,
        tags: None,
        detail: item.detail,
        uri,
        range: full_range.local_range(),
        selection_range: selection_range.local_range(),
        // The `data` field is intentionally unused. We re-derive identity from
        // `(uri, selection_range.start)` — see `resolve_item_location`.
        data: None,
    })
}
