use lsp_types::request::CallHierarchyOutgoingCalls;
use lsp_types::{CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams};

use crate::server::api::call_hierarchy::outgoing_calls_handler;
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
    type RequestType = CallHierarchyOutgoingCalls;
}

impl BackgroundRequestHandler for CallHierarchyOutgoingCallsRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: CallHierarchyOutgoingCallsParams,
    ) -> crate::server::Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        Ok(outgoing_calls_handler(snapshot, &params.item))
    }
}

impl RetriableRequestHandler for CallHierarchyOutgoingCallsRequestHandler {}
