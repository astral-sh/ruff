//! Testing server for the ty language server.
//!
//! This module provides mock server infrastructure for testing LSP functionality using a
//! temporary directory on the real filesystem.
//!
//! The design is inspired by the Starlark LSP test server but adapted for ty server architecture.
//!
//! To get started, use the [`TestServerBuilder`] to configure the server with workspace folders,
//! enable or disable specific client capabilities, and add test files. Then, use the [`build`]
//! method to create the [`TestServer`]. This will start the server and perform the initialization
//! handshake.
//!
//! Once the setup is done, you can use the server to [`send_request`] and [`send_notification`] to
//! send messages to the server and [`await_response`], [`await_request`], and
//! [`await_notification`] to wait for responses, requests, and notifications from the server.
//!
//! The [`Drop`] implementation of the [`TestServer`] ensures that the server is shut down
//! gracefully using the LSP protocol. It also asserts that all messages sent by the server
//! have been handled by the test client before the server is dropped.
//!
//! [`build`]: TestServerBuilder::build
//! [`send_request`]: TestServer::send_request
//! [`send_notification`]: TestServer::send_notification
//! [`await_response`]: TestServer::await_response
//! [`await_request`]: TestServer::await_request
//! [`await_notification`]: TestServer::await_notification

mod code_action;
mod custom_extension;
mod hover;
mod notebook;
mod workspace;

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{fmt, fs};

use anyhow::{Context, Result, anyhow};
use crossbeam::channel::RecvTimeoutError;
use insta::internals::SettingsBindDropGuard;
use lsp_server::{Connection, Message, RequestId, Response, ResponseError};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders, DidCloseTextDocument,
    DidOpenTextDocument, Exit, Initialized, Notification,
};
use lsp_types::request::{
    CodeActionRequest, DocumentDiagnosticRequest, HoverRequest, Initialize, Request, Shutdown,
};
use lsp_types::{
    ClientCapabilities, CodeActionContext, CodeActionParams, CodeActionResponse,
    DiagnosticClientCapabilities, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentDiagnosticParams, DocumentDiagnosticReportResult, FileEvent, Hover, HoverParams,
    InitializeParams, InitializeResult, InitializedParams, NumberOrString, PartialResultParams,
    Position, PublishDiagnosticsClientCapabilities, Range, TextDocumentClientCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextEdit, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams, WorkspaceClientCapabilities, WorkspaceFolder,
    WorkspaceFoldersChangeEvent,
};
use ruff_server::{ConnectionInitializer, LogLevel, Server, init_logging};
use rustc_hash::FxHashMap;
use tempfile::TempDir;

/// Number of times to retry receiving a message before giving up
const RETRY_COUNT: usize = 5;

static INIT_TRACING: OnceLock<()> = OnceLock::new();

/// Setup tracing for the test server.
///
/// This will make sure that the tracing subscriber is initialized only once, so that running
/// multiple tests does not cause multiple subscribers to be registered.
fn setup_tracing() {
    INIT_TRACING.get_or_init(|| {
        init_logging(LogLevel::Debug, None);
    });
}

/// Errors when receiving a notification or request from the server.
#[derive(thiserror::Error, Debug)]
pub(crate) enum ServerMessageError {
    #[error("waiting for message timed out")]
    Timeout,

    #[error("server disconnected")]
    ServerDisconnected,

    #[error("Failed to deserialize message body: {0}")]
    DeserializationError(#[from] serde_json::Error),
}

impl From<ReceiveError> for ServerMessageError {
    fn from(value: ReceiveError) -> Self {
        match value {
            ReceiveError::Timeout => Self::Timeout,
            ReceiveError::ServerDisconnected => Self::ServerDisconnected,
        }
    }
}

/// Errors when receiving a response from the server.
#[derive(thiserror::Error, Debug)]
pub(crate) enum AwaitResponseError {
    /// The response came back, but was an error response, not a successful one.
    #[error("request failed because the server replied with an error: {0:?}")]
    RequestFailed(ResponseError),

    #[error("malformed response message with both result and error: {0:#?}")]
    MalformedResponse(Box<Response>),

    #[error("received multiple responses for the same request ID: {0:#?}")]
    MultipleResponses(Box<[Response]>),

    #[error("waiting for response timed out")]
    Timeout,

    #[error("server disconnected")]
    ServerDisconnected,

    #[error("failed to deserialize response result: {0}")]
    DeserializationError(#[from] serde_json::Error),
}

impl From<ReceiveError> for AwaitResponseError {
    fn from(err: ReceiveError) -> Self {
        match err {
            ReceiveError::Timeout => Self::Timeout,
            ReceiveError::ServerDisconnected => Self::ServerDisconnected,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ReceiveError {
    #[error("waiting for message timed out")]
    Timeout,

    #[error("server disconnected")]
    ServerDisconnected,
}

/// A test server for the ty language server that provides helpers for sending requests,
/// correlating responses, and handling notifications.
pub(crate) struct TestServer {
    /// The thread that's actually running the server.
    ///
    /// This is an [`Option`] so that the join handle can be taken out when the server is dropped,
    /// allowing the server thread to be joined and cleaned up properly.
    server_thread: Option<JoinHandle<()>>,

    /// Connection to communicate with the server.
    ///
    /// This is an [`Option`] so that it can be taken out when the server is dropped, allowing
    /// the connection to be cleaned up properly.
    client_connection: Option<Connection>,

    /// Test context that provides the project root directory that holds all test files.
    ///
    /// This directory is automatically cleaned up when the [`TestServer`] is dropped.
    test_context: TestContext,

    /// Incrementing counter to automatically generate request IDs
    request_counter: i32,

    /// A mapping of request IDs to responses received from the server.
    ///
    /// Valid responses contain exactly one response but may contain multiple responses
    /// when the server sends multiple responses for a single request.
    /// The responses are guaranteed to never be empty.
    responses: FxHashMap<RequestId, smallvec::SmallVec<[Response; 1]>>,

    /// An ordered queue of all the notifications received from the server
    notifications: VecDeque<lsp_server::Notification>,

    /// An ordered queue of all the requests received from the server
    requests: VecDeque<lsp_server::Request>,

    /// The response from server initialization
    initialize_response: Option<InitializeResult>,

    /// Whether a Shutdown request has been sent by the test
    /// and the exit sequence should be skipped during `Drop`
    shutdown_requested: bool,
}

impl TestServer {
    /// Create a new test server with the given workspace configurations
    fn new(
        workspaces: Vec<WorkspaceFolder>,
        test_context: TestContext,
        capabilities: ClientCapabilities,
        initialization_options: Option<serde_json::Value>,
    ) -> Self {
        setup_tracing();

        tracing::debug!("Starting test client with capabilities {:#?}", capabilities);

        let (server_connection, client_connection) = ConnectionInitializer::memory();
        // Start the server in a separate thread
        let server_thread = std::thread::spawn(move || {
            // TODO: This should probably be configurable to test concurrency issues
            let worker_threads = NonZeroUsize::new(1).unwrap();

            match Server::new(worker_threads, server_connection, None, true) {
                Ok(server) => {
                    if let Err(err) = server.run() {
                        panic!("Server stopped with error: {err:?}");
                    }
                }
                Err(err) => {
                    panic!("Failed to create server: {err:?}");
                }
            }
        });

        Self {
            server_thread: Some(server_thread),
            client_connection: Some(client_connection),
            test_context,
            request_counter: 0,
            responses: FxHashMap::default(),
            notifications: VecDeque::new(),
            requests: VecDeque::new(),
            initialize_response: None,
            shutdown_requested: false,
        }
        .initialize(workspaces, capabilities, initialization_options)
    }

    /// Perform LSP initialization handshake
    ///
    /// # Panics
    ///
    /// If the `initialization_options` cannot be serialized to JSON
    fn initialize(
        mut self,
        workspace_folders: Vec<WorkspaceFolder>,
        capabilities: ClientCapabilities,
        initialization_options: Option<serde_json::Value>,
    ) -> Self {
        let init_params = InitializeParams {
            capabilities,
            workspace_folders: Some(workspace_folders),
            initialization_options,
            ..Default::default()
        };

        let init_request_id = self.send_request::<Initialize>(init_params);
        self.initialize_response = Some(self.await_response::<Initialize>(&init_request_id));
        self.send_notification::<Initialized>(InitializedParams {});

        self
    }

    /// Drain all messages from the server.
    fn drain_messages(&mut self) {
        // Don't wait too long to drain the messages, as this is called in the `Drop`
        // implementation which happens everytime the test ends.
        while let Ok(()) = self.receive(Some(Duration::from_millis(10))) {}
    }

    /// Validate that there are no pending messages from the server.
    ///
    /// This should be called before the test server is dropped to ensure that all server messages
    /// have been properly consumed by the test. If there are any pending messages, this will panic
    /// with detailed information about what was left unconsumed.
    #[track_caller]
    fn assert_no_pending_messages(&self) {
        let mut errors = Vec::new();

        if !self.responses.is_empty() {
            errors.push(format!("Unclaimed responses: {:#?}", self.responses));
        }

        if !self.notifications.is_empty() {
            errors.push(format!(
                "Unclaimed notifications: {:#?}",
                self.notifications
            ));
        }

        if !self.requests.is_empty() {
            errors.push(format!("Unclaimed requests: {:#?}", self.requests));
        }

        assert!(
            errors.is_empty(),
            "Test server has pending messages that were not consumed by the test:\n{}",
            errors.join("\n")
        );
    }

    /// Generate a new request ID
    fn next_request_id(&mut self) -> RequestId {
        self.request_counter += 1;
        RequestId::from(self.request_counter)
    }

    /// Send a message to the server.
    ///
    /// # Panics
    ///
    /// If the server is still running but the client connection got dropped, or if the server
    /// exited unexpectedly or panicked.
    #[track_caller]
    fn send(&mut self, message: Message) {
        if self
            .client_connection
            .as_ref()
            .unwrap()
            .sender
            .send(message)
            .is_err()
        {
            self.panic_on_server_disconnect();
        }
    }

    /// Send a request to the server and return the request ID.
    ///
    /// The caller can use this ID to later retrieve the response using [`await_response`].
    ///
    /// [`await_response`]: TestServer::await_response
    pub(crate) fn send_request<R>(&mut self, params: R::Params) -> RequestId
    where
        R: Request,
    {
        // Track if an Exit notification is being sent
        if R::METHOD == lsp_types::request::Shutdown::METHOD {
            self.shutdown_requested = true;
        }

        let id = self.next_request_id();
        tracing::debug!("Client sends request `{}` with ID {}", R::METHOD, id);
        let request = lsp_server::Request::new(id.clone(), R::METHOD.to_string(), params);
        self.send(Message::Request(request));
        id
    }

    /// Send a notification to the server.
    pub(crate) fn send_notification<N>(&mut self, params: N::Params)
    where
        N: Notification,
    {
        let notification = lsp_server::Notification::new(N::METHOD.to_string(), params);
        tracing::debug!("Client sends notification `{}`", N::METHOD);
        self.send(Message::Notification(notification));
    }

    /// Wait for a server response corresponding to the given request ID.
    ///
    /// This should only be called if a request was already sent to the server via [`send_request`]
    /// which returns the request ID that should be used here.
    ///
    /// This method will remove the response from the internal data structure, so it can only be
    /// called once per request ID.
    ///
    /// # Panics
    ///
    /// If the server didn't send a response, the response failed with an error code, failed to deserialize,
    /// or the server responded twice. Use [`Self::try_await_response`] if you want a non-panicking version.
    ///
    /// [`send_request`]: TestServer::send_request
    #[track_caller]
    pub(crate) fn await_response<R>(&mut self, id: &RequestId) -> R::Result
    where
        R: Request,
    {
        self.try_await_response::<R>(id, None)
            .unwrap_or_else(|err| panic!("Failed to receive response for request {id}: {err}"))
    }

    #[expect(dead_code)]
    #[track_caller]
    pub(crate) fn send_request_await<R>(&mut self, params: R::Params) -> R::Result
    where
        R: Request,
    {
        let id = self.send_request::<R>(params);
        self.try_await_response::<R>(&id, None)
            .unwrap_or_else(|err| panic!("Failed to receive response for request {id}: {err}"))
    }

    /// Wait for a server response corresponding to the given request ID.
    ///
    /// This should only be called if a request was already sent to the server via [`send_request`]
    /// which returns the request ID that should be used here.
    ///
    /// This method will remove the response from the internal data structure, so it can only be
    /// called once per request ID.
    ///
    /// [`send_request`]: TestServer::send_request
    pub(crate) fn try_await_response<R>(
        &mut self,
        id: &RequestId,
        timeout: Option<Duration>,
    ) -> Result<R::Result, AwaitResponseError>
    where
        R: Request,
    {
        loop {
            if let Some(mut responses) = self.responses.remove(id) {
                if responses.len() > 1 {
                    return Err(AwaitResponseError::MultipleResponses(
                        responses.into_boxed_slice(),
                    ));
                }

                let response = responses.pop().unwrap();

                match response {
                    Response {
                        error: None,
                        result: Some(result),
                        ..
                    } => {
                        return Ok(serde_json::from_value::<R::Result>(result)?);
                    }
                    Response {
                        error: Some(err),
                        result: None,
                        ..
                    } => {
                        return Err(AwaitResponseError::RequestFailed(err));
                    }
                    response => {
                        return Err(AwaitResponseError::MalformedResponse(Box::new(response)));
                    }
                }
            }

            self.receive(timeout)?;
        }
    }

    /// Wait for a notification of the specified type from the server and return its parameters.
    ///
    /// The caller should ensure that the server is expected to send this notification type. It
    /// will keep polling the server for this notification up to 10 times before giving up after
    /// which it will return an error. It will also return an error if the notification is not
    /// received within `recv_timeout` duration.
    ///
    /// This method will remove the notification from the internal data structure, so it should
    /// only be called if the notification is expected to be sent by the server.
    ///
    /// # Panics
    ///
    /// If the server doesn't send the notification within the default timeout or
    /// the notification failed to deserialize. Use [`Self::try_await_notification`] for
    /// a panic-free alternative.
    #[track_caller]
    pub(crate) fn await_notification<N: Notification>(&mut self) -> N::Params {
        match self.try_await_notification::<N>(None) {
            Ok(result) => result,
            Err(err) => {
                panic!("Failed to receive notification `{}`: {err}", N::METHOD)
            }
        }
    }

    /// Wait for a notification of the specified type from the server and return its parameters.
    ///
    /// The caller should ensure that the server is expected to send this notification type. It
    /// will keep polling the server for this notification up to 10 times before giving up after
    /// which it will return an error. It will also return an error if the notification is not
    /// received within `recv_timeout` duration.
    ///
    /// This method will remove the notification from the internal data structure, so it should
    /// only be called if the notification is expected to be sent by the server.
    pub(crate) fn try_await_notification<N: Notification>(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<N::Params, ServerMessageError> {
        for retry_count in 0..RETRY_COUNT {
            if retry_count > 0 {
                tracing::info!("Retrying to receive `{}` notification", N::METHOD);
            }
            let notification = self
                .notifications
                .iter()
                .position(|notification| N::METHOD == notification.method)
                .and_then(|index| self.notifications.remove(index));
            if let Some(notification) = notification {
                let params = serde_json::from_value(notification.params)?;
                return Ok(params);
            }

            self.receive(timeout)?;
        }

        Err(ServerMessageError::Timeout)
    }

    /// Collects `N` publish diagnostic notifications into a map, indexed by the document url.
    ///
    /// ## Panics
    /// If there are multiple publish diagnostics notifications for the same document.
    #[track_caller]
    pub(crate) fn collect_publish_diagnostic_notifications(
        &mut self,
        count: usize,
    ) -> BTreeMap<lsp_types::Url, Vec<lsp_types::Diagnostic>> {
        let mut results = BTreeMap::default();

        for _ in 0..count {
            let notification =
                self.await_notification::<lsp_types::notification::PublishDiagnostics>();

            if let Some(existing) =
                results.insert(notification.uri.clone(), notification.diagnostics)
            {
                panic!(
                    "Received multiple publish diagnostic notifications for {url}: ({existing:#?})",
                    url = &notification.uri
                );
            }
        }

        results
    }

    /// Wait for a request of the specified type from the server and return the request ID and
    /// parameters.
    ///
    /// The caller should ensure that the server is expected to send this request type. It will
    /// keep polling the server for this request up to 10 times before giving up after which it
    /// will return an error. It can also return an error if the request is not received within
    /// `recv_timeout` duration.
    ///
    /// This method will remove the request from the internal data structure, so it should only be
    /// called if the request is expected to be sent by the server.
    ///
    /// # Panics
    ///
    /// If receiving the request fails.
    #[track_caller]
    #[expect(dead_code)]
    pub(crate) fn await_request<R: Request>(&mut self) -> (RequestId, R::Params) {
        match self.try_await_request::<R>(None) {
            Ok(result) => result,
            Err(err) => {
                panic!("Failed to receive server request `{}`: {err}", R::METHOD)
            }
        }
    }

    /// Wait for a request of the specified type from the server and return the request ID and
    /// parameters.
    ///
    /// The caller should ensure that the server is expected to send this request type. It will
    /// keep polling the server for this request up to 10 times before giving up after which it
    /// will return an error. It can also return an error if the request is not received within
    /// `recv_timeout` duration.
    ///
    /// This method will remove the request from the internal data structure, so it should only be
    /// called if the request is expected to be sent by the server.
    #[track_caller]
    pub(crate) fn try_await_request<R: Request>(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<(RequestId, R::Params), ServerMessageError> {
        for retry_count in 0..RETRY_COUNT {
            if retry_count > 0 {
                tracing::info!("Retrying to receive `{}` request", R::METHOD);
            }
            let request = self
                .requests
                .iter()
                .position(|request| R::METHOD == request.method)
                .and_then(|index| self.requests.remove(index));
            if let Some(request) = request {
                let params = serde_json::from_value(request.params)?;
                return Ok((request.id, params));
            }

            self.receive(timeout)?;
        }
        Err(ServerMessageError::Timeout)
    }

    /// Receive a message from the server.
    ///
    /// It will wait for `timeout` duration for a message to arrive. If no message is received
    /// within that time, it will return an error.
    ///
    /// If `timeout` is `None`, it will use a default timeout of 10 second.
    fn receive(&mut self, timeout: Option<Duration>) -> Result<(), ReceiveError> {
        static DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

        let receiver = self.client_connection.as_ref().unwrap().receiver.clone();
        let message = receiver
            .recv_timeout(timeout.unwrap_or(DEFAULT_TIMEOUT))
            .map_err(|err| match err {
                RecvTimeoutError::Disconnected => ReceiveError::ServerDisconnected,
                RecvTimeoutError::Timeout => ReceiveError::Timeout,
            })?;

        self.handle_message(message);

        for message in receiver.try_iter() {
            self.handle_message(message);
        }

        Ok(())
    }

    /// Handle the incoming message from the server.
    ///
    /// This method will store the message as follows:
    /// - Requests are stored in `self.requests`
    /// - Responses are stored in `self.responses` with the request ID as the key
    /// - Notifications are stored in `self.notifications`
    fn handle_message(&mut self, message: Message) {
        match message {
            Message::Request(request) => {
                tracing::debug!("Received server request `{}`", &request.method);
                self.requests.push_back(request);
            }
            Message::Response(response) => {
                tracing::debug!("Received server response for request {}", &response.id);
                self.responses
                    .entry(response.id.clone())
                    .or_default()
                    .push(response);
            }
            Message::Notification(notification) => {
                tracing::debug!("Received notification `{}`", &notification.method);
                self.notifications.push_back(notification);
            }
        }
    }

    #[track_caller]
    fn panic_on_server_disconnect(&mut self) -> ! {
        if let Some(handle) = &self.server_thread {
            if handle.is_finished() {
                let handle = self.server_thread.take().unwrap();
                if let Err(panic) = handle.join() {
                    std::panic::resume_unwind(panic);
                }
                panic!("Server exited unexpectedly");
            }
        }

        panic!("Server dropped client receiver while still running");
    }

    #[expect(dead_code)]
    pub(crate) fn cancel(&mut self, request_id: &RequestId) {
        let id_string = request_id.to_string();
        self.send_notification::<lsp_types::notification::Cancel>(lsp_types::CancelParams {
            id: match id_string.parse() {
                Ok(id) => NumberOrString::Number(id),
                Err(_) => NumberOrString::String(id_string),
            },
        });
    }

    /// Get the initialization result
    #[expect(dead_code)]
    pub(crate) fn initialization_result(&self) -> Option<&InitializeResult> {
        self.initialize_response.as_ref()
    }

    pub(crate) fn file_uri(&self, path: impl AsRef<Path>) -> Url {
        Url::from_file_path(self.file_path(path)).expect("Path must be a valid URL")
    }

    pub(crate) fn file_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.test_context.root().join(path)
    }

    #[expect(dead_code)]
    pub(crate) fn write_file(
        &self,
        path: impl AsRef<Path>,
        content: impl AsRef<str>,
    ) -> Result<()> {
        let file_path = self.file_path(path);
        // Ensure parent directories exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file_path, content.as_ref())?;
        Ok(())
    }

    /// Send a `textDocument/didOpen` notification
    pub(crate) fn open_text_document(
        &mut self,
        path: impl AsRef<Path>,
        content: impl AsRef<str>,
        version: i32,
    ) {
        self.open_text_document_with_language_id(path, "python", content, version);
    }

    /// Send a `textDocument/didOpen` notification with the specified language id.
    pub(crate) fn open_text_document_with_language_id(
        &mut self,
        path: impl AsRef<Path>,
        language_id: &str,
        content: impl AsRef<str>,
        version: i32,
    ) {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: self.file_uri(path),
                language_id: language_id.to_string(),
                version,
                text: content.as_ref().to_string(),
            },
        };
        self.send_notification::<DidOpenTextDocument>(params);
    }

    /// Send a `textDocument/didChange` notification with the given content changes
    #[expect(dead_code)]
    pub(crate) fn change_text_document(
        &mut self,
        path: impl AsRef<Path>,
        changes: Vec<TextDocumentContentChangeEvent>,
        version: i32,
    ) {
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: self.file_uri(path),
                version,
            },
            content_changes: changes,
        };
        self.send_notification::<DidChangeTextDocument>(params);
    }

    /// Send a `textDocument/didClose` notification
    #[expect(dead_code)]
    pub(crate) fn close_text_document(&mut self, path: impl AsRef<Path>) {
        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: self.file_uri(path),
            },
        };
        self.send_notification::<DidCloseTextDocument>(params);
    }

    /// Send a `workspace/didChangeWatchedFiles` notification with the given file events
    #[expect(dead_code)]
    pub(crate) fn did_change_watched_files(&mut self, events: Vec<FileEvent>) {
        let params = DidChangeWatchedFilesParams { changes: events };
        self.send_notification::<DidChangeWatchedFiles>(params);
    }

    /// Send a `workspace/didChangeWorkspaceFolders` notification with the given added/removed
    /// workspace folders. The paths provided should be paths to the root of the workspace folder.
    #[expect(dead_code)]
    pub(crate) fn change_workspace_folders<P: AsRef<Path>>(
        &mut self,
        added: impl IntoIterator<Item = P>,
        removed: impl IntoIterator<Item = P>,
    ) {
        let path_to_workspace_folder = |path: &Path| -> WorkspaceFolder {
            let uri = self.file_uri(path);
            WorkspaceFolder {
                uri,
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_string(),
            }
        };
        let params = DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: added
                    .into_iter()
                    .map(|path| path_to_workspace_folder(path.as_ref()))
                    .collect(),
                removed: removed
                    .into_iter()
                    .map(|path| path_to_workspace_folder(path.as_ref()))
                    .collect(),
            },
        };
        self.send_notification::<DidChangeWorkspaceFolders>(params);
    }

    /// Send a `textDocument/diagnostic` request for the document at the given path.
    pub(crate) fn document_diagnostic_request(
        &mut self,
        path: impl AsRef<Path>,
        previous_result_id: Option<String>,
    ) -> DocumentDiagnosticReportResult {
        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: self.file_uri(path),
            },
            identifier: Some("ty".to_string()),
            previous_result_id,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let id = self.send_request::<DocumentDiagnosticRequest>(params);
        self.await_response::<DocumentDiagnosticRequest>(&id)
    }

    /// Send a `textDocument/hover` request for the document at the given path and position.
    pub(crate) fn hover_request(
        &mut self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Option<Hover> {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: self.file_uri(path),
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let id = self.send_request::<HoverRequest>(params);
        self.await_response::<HoverRequest>(&id)
    }

    /// Send a `textDocument/formatting` request for the document at the given path.
    pub(crate) fn format_request(&mut self, path: impl AsRef<Path>) -> Option<Vec<TextEdit>> {
        let id = self.send_request::<lsp_types::request::Formatting>(
            lsp_types::DocumentFormattingParams {
                text_document: TextDocumentIdentifier {
                    uri: self.file_uri(path),
                },
                options: lsp_types::FormattingOptions::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        );

        self.await_response::<lsp_types::request::Formatting>(&id)
    }

    /// Send a `textDocument/rangeFormatting` request for the document at the given path.
    pub(crate) fn format_range_request(
        &mut self,
        path: impl AsRef<Path>,
        range: Range,
    ) -> Option<Vec<TextEdit>> {
        let id = self.send_request::<lsp_types::request::RangeFormatting>(
            lsp_types::DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier {
                    uri: self.file_uri(path),
                },
                range,
                options: lsp_types::FormattingOptions::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        );
        self.await_response::<lsp_types::request::RangeFormatting>(&id)
    }

    /// Send a `textDocument/codeAction` request for the document at the given path.
    pub(crate) fn code_action_request(
        &mut self,
        path: impl AsRef<Path>,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> Option<CodeActionResponse> {
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier {
                uri: self.file_uri(path),
            },
            range: lsp_types::Range::default(),
            context: CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let id = self.send_request::<CodeActionRequest>(params);
        self.await_response::<CodeActionRequest>(&id)
    }

    #[expect(dead_code)]
    pub(crate) fn respond(&mut self, request_id: RequestId, result: impl serde::Serialize) {
        let response = Response::new_ok(request_id, result);
        self.send(Message::Response(response));
    }
}

impl fmt::Debug for TestServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TestServer")
            .field("temp_dir", &self.test_context.root())
            .field("request_counter", &self.request_counter)
            .field("responses", &self.responses)
            .field("notifications", &self.notifications)
            .field("server_requests", &self.requests)
            .field("initialize_response", &self.initialize_response)
            .finish_non_exhaustive()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.drain_messages();

        // Follow the LSP protocol to shutdown the server gracefully.
        //
        // The `server_thread` could be `None` if the server exited unexpectedly or panicked or if
        // it dropped the client connection.
        let shutdown_error = if self.server_thread.is_some() && !self.shutdown_requested {
            let shutdown_id = self.send_request::<Shutdown>(());
            match self.try_await_response::<Shutdown>(&shutdown_id, None) {
                Ok(()) => {
                    self.send_notification::<Exit>(());

                    None
                }
                Err(err) => Some(format!("Failed to get shutdown response: {err:?}")),
            }
        } else {
            None
        };

        // Drop the client connection before joining the server thread to avoid any hangs
        // in case the server didn't respond to the shutdown request.
        if let Some(client_connection) = self.client_connection.take() {
            if !std::thread::panicking() {
                // Wait for the client sender to drop (confirmation that it processed the exit notification).

                match client_connection
                    .receiver
                    .recv_timeout(Duration::from_secs(20))
                {
                    Err(RecvTimeoutError::Disconnected) => {
                        // Good, the server terminated
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        tracing::warn!(
                            "The server didn't exit within 20ms after receiving the EXIT notification"
                        );
                    }
                    Ok(message) => {
                        self.handle_message(message);
                    }
                }
            }
        }

        if std::thread::panicking() {
            // If the test server panicked, avoid further assertions.
            return;
        }

        if let Some(server_thread) = self.server_thread.take() {
            if let Err(err) = server_thread.join() {
                panic!("Panic in the server thread: {err:?}");
            }
        }

        if let Some(error) = shutdown_error {
            panic!("Test server did not shut down gracefully: {error}");
        }

        self.assert_no_pending_messages();
    }
}

/// Builder for creating test servers with specific configurations
pub(crate) struct TestServerBuilder {
    test_context: TestContext,
    workspaces: Vec<WorkspaceFolder>,
    initialization_options: Option<serde_json::Value>,
    client_capabilities: ClientCapabilities,
}

impl TestServerBuilder {
    /// Create a new builder
    pub(crate) fn new() -> Result<Self> {
        // Default client capabilities for the test server:
        //
        // These are common capabilities that all clients support:
        // - Supports publishing diagnostics
        //
        // These are enabled by default for convenience but can be disabled using the builder
        // methods:
        // - Supports pulling workspace configuration
        // - Support for pull diagnostics
        let client_capabilities = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
                // Most clients support pull diagnostics and they're easier to work for in tests because
                // it doesn't require consuming the publish notifications in each test.
                diagnostic: Some(DiagnosticClientCapabilities::default()),
                ..Default::default()
            }),
            workspace: Some(WorkspaceClientCapabilities {
                configuration: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };

        Ok(Self {
            workspaces: Vec::new(),
            test_context: TestContext::new()?,
            initialization_options: None,
            client_capabilities,
        })
    }

    /// Set the initial client options for the test server
    #[expect(dead_code)]
    pub(crate) fn with_initialization_options(mut self, options: serde_json::Value) -> Self {
        self.initialization_options = Some(options);
        self
    }

    /// Add a workspace to the test server with the given root path.
    pub(crate) fn with_workspace(mut self, workspace_root: impl AsRef<Path>) -> Result<Self> {
        let workspace_root = workspace_root.as_ref();
        let workspace_path = self.test_context.root().join(workspace_root);
        fs::create_dir_all(&workspace_path)?;

        self.workspaces.push(WorkspaceFolder {
            uri: Url::from_file_path(&workspace_path).map_err(|()| {
                anyhow!(
                    "Failed to convert workspace path to URL: {}",
                    workspace_path.display()
                )
            })?,
            name: workspace_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("test")
                .to_string(),
        });

        Ok(self)
    }

    /// Enable or disable pull diagnostics capability
    #[expect(dead_code)]
    pub(crate) fn enable_pull_diagnostics(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .text_document
            .get_or_insert_default()
            .diagnostic = if enabled {
            Some(DiagnosticClientCapabilities::default())
        } else {
            None
        };
        self
    }

    /// Enable or disable dynamic registration of diagnostics capability
    #[expect(dead_code)]
    pub(crate) fn enable_diagnostic_dynamic_registration(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .text_document
            .get_or_insert_default()
            .diagnostic
            .get_or_insert_default()
            .dynamic_registration = Some(enabled);
        self
    }

    /// Enable or disable workspace configuration capability
    #[expect(dead_code)]
    pub(crate) fn enable_workspace_configuration(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .workspace
            .get_or_insert_default()
            .configuration = Some(enabled);
        self
    }

    #[expect(dead_code)]
    pub(crate) fn enable_diagnostic_related_information(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .text_document
            .get_or_insert_default()
            .publish_diagnostics
            .get_or_insert_default()
            .related_information = Some(enabled);
        self
    }

    /// Set custom client capabilities (overrides any previously set capabilities)
    #[expect(dead_code)]
    pub(crate) fn with_client_capabilities(mut self, capabilities: ClientCapabilities) -> Self {
        self.client_capabilities = capabilities;
        self
    }

    pub(crate) fn file_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.test_context.root().join(path)
    }

    /// Write a file to the test directory
    pub(crate) fn with_file(
        self,
        path: impl AsRef<Path>,
        content: impl AsRef<str>,
    ) -> Result<Self> {
        let file_path = self.file_path(path);
        // Ensure parent directories exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file_path, content.as_ref())?;
        Ok(self)
    }

    /// Write multiple files to the test directory
    #[expect(dead_code)]
    pub(crate) fn with_files<P, C, I>(mut self, files: I) -> Result<Self>
    where
        I: IntoIterator<Item = (P, C)>,
        P: AsRef<Path>,
        C: AsRef<str>,
    {
        for (path, content) in files {
            self = self.with_file(path, content)?;
        }
        Ok(self)
    }

    /// Build the test server
    pub(crate) fn build(self) -> TestServer {
        TestServer::new(
            self.workspaces,
            self.test_context,
            self.client_capabilities,
            self.initialization_options,
        )
    }
}

/// A context specific to a server test.
///
/// This creates a temporary directory that is used as the current working directory for the server
/// in which the test files are stored. This also holds the insta settings scope that filters out
/// the temporary directory path from snapshots.
///
/// This is similar to the `CliTest` in `ty` crate.
struct TestContext {
    _temp_dir: TempDir,
    _settings_scope: SettingsBindDropGuard,
    project_dir: PathBuf,
}

impl TestContext {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macos uses symlinks for tempdirs
        // and that doesn't play well with our snapshot filtering.
        // Simplify with dunce because otherwise we get UNC paths on Windows.
        let project_dir = dunce::simplified(
            &temp_dir
                .path()
                .canonicalize()
                .context("Failed to canonicalize project path")?,
        )
        .to_path_buf();

        let mut settings = insta::Settings::clone_current();
        let project_dir_url = Url::from_file_path(&project_dir)
            .map_err(|()| anyhow!("Failed to convert root directory to url"))?;
        settings.add_filter(
            &tempdir_filter(project_dir.to_string_lossy().as_ref()),
            "<temp_dir>/",
        );
        settings.add_filter(&tempdir_filter(project_dir_url.path()), "<temp_dir>/");
        settings.add_filter(
            r#"The system cannot find the file specified."#,
            "No such file or directory",
        );

        let settings_scope = settings.bind_to_scope();

        Ok(Self {
            project_dir,
            _temp_dir: temp_dir,
            _settings_scope: settings_scope,
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.project_dir
    }
}

fn tempdir_filter(path: impl AsRef<str>) -> String {
    format!(r"{}\\?/?", regex::escape(path.as_ref()))
}
