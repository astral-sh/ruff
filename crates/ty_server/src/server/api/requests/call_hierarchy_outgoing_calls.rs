use lsp_types::CallHierarchyOutgoingCallsRequest;
use lsp_types::{CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams};
use ruff_db::PythonFile;
use ty_project::Db as _;

use crate::document::{ToRangeExt as _, resolve_file_uri_range};
use crate::server::api::requests::prepare_call_hierarchy::convert_to_lsp_item;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

/// Handles `callHierarchy/outgoingCalls`.
///
/// Implements `BackgroundRequestHandler` rather than the document variant
/// because the prepared item may live in a file that is not open in the
/// current session — the request still has to work.
pub(crate) struct CallHierarchyOutgoingCallsRequestHandler;

impl RequestHandler for CallHierarchyOutgoingCallsRequestHandler {
    type RequestType = CallHierarchyOutgoingCallsRequest;
}

impl BackgroundRequestHandler for CallHierarchyOutgoingCallsRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: CallHierarchyOutgoingCallsParams,
    ) -> crate::server::Result<Option<Vec<CallHierarchyOutgoingCall>>> {
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
                ty_ide::outgoing_calls(db, PythonFile::new(db, file, db.python_version()), offset)
            {
                let Some(to) = convert_to_lsp_item(db, call.to, encoding) else {
                    continue;
                };
                let from_ranges: Vec<_> = call
                    .from_ranges
                    .into_iter()
                    .filter_map(|range| range.to_lsp_range(db, file, encoding))
                    .map(|file_range| file_range.local_range())
                    .collect();
                calls.push(CallHierarchyOutgoingCall { to, from_ranges });
            }
        }
        if calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(calls))
        }
    }
}

impl RetriableRequestHandler for CallHierarchyOutgoingCallsRequestHandler {}
