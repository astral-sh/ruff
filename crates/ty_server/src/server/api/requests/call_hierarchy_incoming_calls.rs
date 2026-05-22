use lsp_types::request::CallHierarchyIncomingCalls;
use lsp_types::{CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams};

use crate::server::api::call_hierarchy::incoming_calls_handler;
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
    type RequestType = CallHierarchyIncomingCalls;
}

impl BackgroundRequestHandler for CallHierarchyIncomingCallsRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: CallHierarchyIncomingCallsParams,
    ) -> crate::server::Result<Option<Vec<CallHierarchyIncomingCall>>> {
        Ok(incoming_calls_handler(snapshot, &params.item))
    }
}

impl RetriableRequestHandler for CallHierarchyIncomingCallsRequestHandler {}
