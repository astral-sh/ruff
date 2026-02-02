use crate::session::client::ClientResponseHandler;
use lsp_server::RequestId;
use rustc_hash::FxHashMap;
use std::cell::{Cell, OnceCell, RefCell};
use std::fmt::Formatter;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Tracks the pending requests between client and server.
pub(crate) struct RequestQueue {
    incoming: Incoming,
    outgoing: Outgoing,
}

impl RequestQueue {
    pub(super) fn new() -> Self {
        Self {
            incoming: Incoming::default(),
            outgoing: Outgoing::default(),
        }
    }

    pub(crate) fn outgoing_mut(&mut self) -> &mut Outgoing {
        &mut self.outgoing
    }

    /// Returns the server to client request queue.
    pub(crate) fn outgoing(&self) -> &Outgoing {
        &self.outgoing
    }

    /// Returns the client to server request queue.
    pub(crate) fn incoming(&self) -> &Incoming {
        &self.incoming
    }

    pub(crate) fn incoming_mut(&mut self) -> &mut Incoming {
        &mut self.incoming
    }
}

/// Requests from client -> server.
///
/// Tracks which requests are pending. Requests that aren't registered are considered completed.
///
/// A request is pending if:
///
/// * it has been registered
/// * it hasn't been cancelled
/// * it hasn't been completed
///
/// Tracking whether a request is pending is required to ensure that the server sends exactly
/// one response for every request as required by the LSP specification.
#[derive(Default, Debug)]
pub(crate) struct Incoming {
    pending: FxHashMap<RequestId, PendingRequest>,
}

impl Incoming {
    /// Registers a new pending request.
    pub(crate) fn register(&mut self, request_id: RequestId, method: String) {
        self.pending.insert(request_id, PendingRequest::new(method));
    }

    /// Cancels the pending request with the given id.
    ///
    /// Returns the method name if the request was still pending, `None` if it was already completed.
    pub(super) fn cancel(&mut self, request_id: &RequestId) -> Option<String> {
        self.pending.remove(request_id).map(|mut pending| {
            if let Some(cancellation_token) = pending.cancellation_token.take() {
                cancellation_token.cancel();
            }
            pending.method
        })
    }

    /// Returns `true` if the request with the given id is still pending.
    pub(crate) fn is_pending(&self, request_id: &RequestId) -> bool {
        self.pending.contains_key(request_id)
    }

    /// Returns the cancellation token for the given request id if the request is still pending.
    pub(crate) fn cancellation_token(
        &self,
        request_id: &RequestId,
    ) -> Option<RequestCancellationToken> {
        let pending = self.pending.get(request_id)?;

        Some(RequestCancellationToken::clone(
            pending
                .cancellation_token
                .get_or_init(RequestCancellationToken::default),
        ))
    }

    /// Marks the request as completed.
    ///
    /// Returns the time when the request was registered and the request method name, or `None` if the request was not pending.
    pub(crate) fn complete(&mut self, request_id: &RequestId) -> Option<(Instant, String)> {
        self.pending
            .remove(request_id)
            .map(|pending| (pending.start_time, pending.method))
    }
}

/// A request from the client to the server that hasn't been responded yet.
#[derive(Debug)]
struct PendingRequest {
    /// The time when the request was registered.
    ///
    /// This does not include the time the request was queued in the main loop before it was registered.
    start_time: Instant,

    /// The method name of the request.
    method: String,

    /// A cancellation token to cancel this request.
    ///
    /// This is only initialized for background requests. Local tasks don't support cancellation (unless retried)
    /// as they're processed immediately after receiving the request; Making it impossible for a
    /// cancellation message to be processed before the task is completed.
    cancellation_token: OnceCell<RequestCancellationToken>,
}

impl PendingRequest {
    fn new(method: String) -> Self {
        Self {
            start_time: Instant::now(),
            method,
            cancellation_token: OnceCell::new(),
        }
    }
}

/// Token to cancel a specific request.
///
/// Can be shared between threads to check for cancellation *after* a request has been scheduled.
#[derive(Debug, Default)]
pub(crate) struct RequestCancellationToken(Arc<AtomicBool>);

impl RequestCancellationToken {
    /// Returns true if the request was cancelled.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Signals that the request should not be processed because it was cancelled.
    fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn clone(this: &Self) -> Self {
        RequestCancellationToken(this.0.clone())
    }
}

/// Requests from server -> client.
#[derive(Default)]
pub(crate) struct Outgoing {
    /// The id of the next request sent from the server to the client.
    next_request_id: Cell<i32>,

    /// A map of request ids to the handlers that process the client-response.
    response_handlers: RefCell<FxHashMap<RequestId, ClientResponseHandler>>,
}

impl Outgoing {
    /// Registers a handler, returns the id for the request.
    #[must_use]
    pub(crate) fn register(&self, handler: ClientResponseHandler) -> RequestId {
        let id = self.next_request_id.get();
        self.next_request_id.set(id + 1);

        self.response_handlers
            .borrow_mut()
            .insert(id.into(), handler);
        id.into()
    }

    /// Marks the request with the given id as complete and returns the handler to process the response.
    ///
    /// Returns `None` if the request was not found.
    #[must_use]
    pub(crate) fn complete(&mut self, request_id: &RequestId) -> Option<ClientResponseHandler> {
        self.response_handlers.get_mut().remove(request_id)
    }
}

impl std::fmt::Debug for Outgoing {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outgoing")
            .field("next_request_id", &self.next_request_id)
            .field("response_handlers", &"<response handlers>")
            .finish()
    }
}
