use lsp_types::CallHierarchyIncomingCallsRequest;
use lsp_types::{CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams};
use ruff_db::PythonFile;
use ty_project::SemanticDb as _;

use crate::document::{ToRangeExt as _, resolve_file_uri_range};
use crate::server::api::requests::prepare_call_hierarchy::convert_to_lsp_item;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

/// Handles `callHierarchy/incomingCalls`.
///
/// Implements `BackgroundRequestHandler` rather than the document variant
/// because the prepared item may live in a file that is not open in the
/// current session — the request still has to work.
pub(crate) struct CallHierarchyIncomingCallsRequestHandler;

impl RequestHandler for CallHierarchyIncomingCallsRequestHandler {
    type RequestType = CallHierarchyIncomingCallsRequest;
}

impl BackgroundRequestHandler for CallHierarchyIncomingCallsRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: CallHierarchyIncomingCallsParams,
    ) -> crate::server::Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let encoding = snapshot.position_encoding();
        let requested_item = &params.item;
        let mut calls = Vec::new();

        for db in snapshot.projects() {
            let Some((file, offset)) = resolve_file_uri_range(
                db,
                &requested_item.uri,
                requested_item.selection_range,
                encoding,
            ) else {
                continue;
            };

            for call in
                ty_ide::incoming_calls(db, PythonFile::new(db, file, db.python_version()), offset)
            {
                // `from_ranges` are byte offsets into `call.from.file` (the caller),
                // NOT into `file` (the prepared/queried symbol). Capture the caller
                // file before moving `call.from` into `convert_to_lsp_item`.
                let caller_file = call.from.file;
                let Some(from) = convert_to_lsp_item(db, call.from, encoding) else {
                    continue;
                };
                let from_ranges: Vec<_> = call
                    .from_ranges
                    .into_iter()
                    .filter_map(|range| range.to_lsp_range(db, caller_file, encoding))
                    .map(|file_range| file_range.local_range())
                    .collect();
                calls.push(CallHierarchyIncomingCall { from, from_ranges });
            }
        }
        if calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(calls))
        }
    }
}

impl RetriableRequestHandler for CallHierarchyIncomingCallsRequestHandler {}
