use crate::capabilities::ResolvedClientCapabilities;
use crate::session::client::Client;
use lsp_types::{
    ProgressParams, ProgressToken, WorkDoneProgressBegin, WorkDoneProgressCreateParams,
    WorkDoneProgressCreateRequest, WorkDoneProgressEnd, WorkDoneProgressReport,
};
use std::fmt::Display;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

static SERVER_WORK_DONE_TOKENS: AtomicUsize = AtomicUsize::new(0);

/// A [work done progress][work-done-progress] that uses the client provided token if available,
/// but falls back to a server initiated progress if supported by the client.
///
/// The LSP specification supports client and server initiated work done progress reporting:
/// * Client: Many requests have a work done progress token or extend `WorkDoneProgressParams`.
///   For those requests, a server can ask clients to start a work done progress report by
///   setting the work done capability for that request in the server's capabilities during initialize.
///   However, as of today (July 2025), VS code and Zed don't support client initiated work done progress
///   tokens except for the `initialize` request (<https://github.com/microsoft/vscode-languageserver-node/issues/528>).
/// * Server: A server can initiate a work done progress report by sending a `WorkDoneProgressCreate` request
///   with a token, which the client can then use to report progress (except during `initialize`).
///
/// This work done progress supports both clients that provide a work done progress token in their requests
/// and clients that do not. If the client does not provide a token, the server will
/// initiate a work done progress report using a unique string token.
///
/// ## Server Initiated Progress
///
/// The implementation initiates a work done progress report lazily when no token is provided in the request.
/// This creation happens async and the LSP specification requires that a server only
/// sends `$/progress` notifications with that token if the create request was successful (no error):
///
/// > code and message set in case an exception happens during the 'window/workDoneProgress/create' request.
/// > In case an error occurs a server must not send any progress notification
/// > using the token provided in the WorkDoneProgressCreateParams.
///
/// The implementation doesn't block on the server response because it feels unfortunate to delay
/// a client request only so that ty can show a progress bar. Therefore, the progress reporting
/// will not be available immediately.
///
/// [work-done-progress]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workDoneProgress
#[derive(Clone)]
pub(crate) struct LazyWorkDoneProgress {
    inner: Arc<Inner>,
}

impl LazyWorkDoneProgress {
    pub(crate) fn new(
        client: &Client,
        request_token: Option<ProgressToken>,
        title: &str,
        capabilities: ResolvedClientCapabilities,
    ) -> Self {
        Self::new_inner(
            client,
            request_token,
            WorkDoneProgressBegin {
                title: title.to_string(),
                cancellable: Some(false),
                message: None,
                percentage: Some(0),
            },
            capabilities,
            ProgressCreation::Queue,
        )
    }

    /// Returns a progress reporter that displays an indicator if the main-loop action queue has
    /// capacity.
    ///
    /// Server-initiated progress requires sending a request to the client through the main loop.
    /// If the queue is full, the progress indicator is not shown. Waiting for capacity would
    /// deadlock because the main loop cannot drain its own queue until this call returns.
    pub(crate) fn new_on_main_loop(
        client: &Client,
        begin: WorkDoneProgressBegin,
        capabilities: ResolvedClientCapabilities,
    ) -> Self {
        Self::new_inner(
            client,
            None,
            begin,
            capabilities,
            ProgressCreation::TryQueue,
        )
    }

    fn new_inner(
        client: &Client,
        request_token: Option<ProgressToken>,
        begin: WorkDoneProgressBegin,
        capabilities: ResolvedClientCapabilities,
        creation: ProgressCreation,
    ) -> Self {
        let work_done = Self {
            inner: Arc::new(Inner {
                token: std::sync::OnceLock::new(),
                finish_message: std::sync::Mutex::default(),
                client: client.clone(),
            }),
        };

        if let Some(token) = request_token {
            if Self::send_begin(client, token.clone(), begin) {
                work_done
                    .inner
                    .token
                    .set(token)
                    .expect("progress token should only be set once");
            }
        } else if capabilities.supports_work_done_progress() {
            // Use a string token because Zed does not support numeric tokens
            let token = ProgressToken::String(format!(
                "ty-{}",
                SERVER_WORK_DONE_TOKENS.fetch_add(1, Ordering::Relaxed)
            ));
            let work_done = work_done.clone();

            let params = WorkDoneProgressCreateParams {
                token: token.clone(),
            };
            let response_handler = move |client: &Client, ()| {
                if Self::send_begin(client, token.clone(), begin) {
                    work_done
                        .inner
                        .token
                        .set(token)
                        .expect("progress token should only be set once");
                }
            };

            match creation {
                ProgressCreation::Queue => client
                    .send_deferred_request::<WorkDoneProgressCreateRequest>(
                        params,
                        response_handler,
                    ),
                ProgressCreation::TryQueue => {
                    client.try_send_deferred_request::<WorkDoneProgressCreateRequest>(
                        params,
                        response_handler,
                    );
                }
            }
        }

        work_done
    }

    pub(super) fn set_finish_message(&self, message: String) {
        let mut finish_message = self.inner.finish_message.lock().unwrap();

        *finish_message = Some(message);
    }

    /// Sends a progress report with the given message and optional percentage.
    ///
    /// Reports sent before the client acknowledges progress creation are dropped.
    pub(super) fn report_progress(&self, message: impl Display, percentage: Option<u32>) {
        let Some(token) = self.inner.token.get() else {
            return;
        };

        self.inner
            .client
            .try_send_notification::<lsp_types::ProgressNotification>(ProgressParams {
                token: token.clone(),
                value: serde_json::to_value(WorkDoneProgressReport {
                    cancellable: Some(false),
                    message: Some(message.to_string()),
                    percentage,
                })
                .expect("Failed to serialize work done progress report"),
            });
    }

    fn send_begin(client: &Client, token: ProgressToken, begin: WorkDoneProgressBegin) -> bool {
        client.try_send_notification::<lsp_types::ProgressNotification>(ProgressParams {
            token,
            value: serde_json::to_value(begin)
                .expect("Failed to serialize work done progress begin"),
        })
    }
}

#[derive(Clone, Copy)]
enum ProgressCreation {
    Queue,
    TryQueue,
}

impl ty_project::UvSyncProgress for LazyWorkDoneProgress {}

struct Inner {
    token: std::sync::OnceLock<ProgressToken>,
    finish_message: std::sync::Mutex<Option<String>>,
    client: Client,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let Some(token) = self.token.get() else {
            return;
        };

        let finish_message = self
            .finish_message
            .lock()
            .ok()
            .and_then(|mut message| message.take());

        self.client
            .send_notification::<lsp_types::ProgressNotification>(ProgressParams {
                token: token.clone(),
                value: serde_json::to_value(WorkDoneProgressEnd {
                    message: finish_message,
                })
                .expect("Failed to serialize work done progress end"),
            });
    }
}
