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
//! handshake. It might be useful to call [`wait_until_workspaces_are_initialized`] to ensure that
//! the server side initialization is complete before sending any requests.
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
//! [`wait_until_workspaces_are_initialized`]: TestServer::wait_until_workspaces_are_initialized
//! [`send_request`]: TestServer::send_request
//! [`send_notification`]: TestServer::send_notification
//! [`await_response`]: TestServer::await_response
//! [`await_request`]: TestServer::await_request
//! [`await_notification`]: TestServer::await_notification

mod commands;
mod initialize;
mod inlay_hints;
mod publish_diagnostics;
mod pull_diagnostics;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{fmt, fs};

use anyhow::{Context, Result, anyhow};
use crossbeam::channel::RecvTimeoutError;
use insta::internals::SettingsBindDropGuard;
use lsp_server::{Connection, Message, RequestId, Response, ResponseError};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument, Exit,
    Initialized, Notification,
};
use lsp_types::request::{
    DocumentDiagnosticRequest, HoverRequest, Initialize, InlayHintRequest, Request, Shutdown,
    WorkspaceConfiguration, WorkspaceDiagnosticRequest,
};
use lsp_types::{
    ClientCapabilities, ConfigurationParams, DiagnosticClientCapabilities,
    DidChangeTextDocumentParams, DidChangeWatchedFilesClientCapabilities,
    DidChangeWatchedFilesParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentDiagnosticParams, DocumentDiagnosticReportResult, FileEvent, Hover, HoverParams,
    InitializeParams, InitializeResult, InitializedParams, InlayHint, InlayHintClientCapabilities,
    InlayHintParams, NumberOrString, PartialResultParams, Position, PreviousResultId,
    PublishDiagnosticsClientCapabilities, Range, TextDocumentClientCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier, WorkDoneProgressParams,
    WorkspaceClientCapabilities, WorkspaceDiagnosticParams, WorkspaceDiagnosticReportResult,
    WorkspaceFolder,
};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf, TestSystem};
use rustc_hash::FxHashMap;
use tempfile::TempDir;

use ty_server::{ClientOptions, LogLevel, Server, init_logging};

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

/// Errors that can occur during testing
#[derive(thiserror::Error, Debug)]
pub(crate) enum TestServerError {
    /// The response came back, but was an error response, not a successful one.
    #[error("Response error: {0:?}")]
    ResponseError(ResponseError),

    #[error("Invalid response message for request {0}: {1:?}")]
    InvalidResponse(RequestId, Box<Response>),

    #[error("Got a duplicate response for request ID {0}: {1:?}")]
    DuplicateResponse(RequestId, Box<Response>),

    #[error("Failed to receive message from server: {0}")]
    RecvTimeoutError(RecvTimeoutError),
}

impl TestServerError {
    fn is_disconnected(&self) -> bool {
        matches!(
            self,
            TestServerError::RecvTimeoutError(RecvTimeoutError::Disconnected)
        )
    }
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

    /// A mapping of request IDs to responses received from the server
    responses: FxHashMap<RequestId, Response>,

    /// An ordered queue of all the notifications received from the server
    notifications: VecDeque<lsp_server::Notification>,

    /// An ordered queue of all the requests received from the server
    requests: VecDeque<lsp_server::Request>,

    /// The response from server initialization
    initialize_response: Option<InitializeResult>,

    /// Workspace configurations for `workspace/configuration` requests
    workspace_configurations: HashMap<Url, ClientOptions>,

    /// Whether a Shutdown request has been sent by the test
    /// and the exit sequence should be skipped during `Drop`
    shutdown_requested: bool,
}

impl TestServer {
    /// Create a new test server with the given workspace configurations
    fn new(
        workspaces: Vec<(WorkspaceFolder, Option<ClientOptions>)>,
        test_context: TestContext,
        capabilities: ClientCapabilities,
        initialization_options: Option<ClientOptions>,
    ) -> Result<Self> {
        setup_tracing();

        let (server_connection, client_connection) = Connection::memory();

        // Create OS system with the test directory as cwd
        let os_system = OsSystem::new(test_context.root());

        // Start the server in a separate thread
        let server_thread = std::thread::spawn(move || {
            // TODO: This should probably be configurable to test concurrency issues
            let worker_threads = NonZeroUsize::new(1).unwrap();
            let test_system = Arc::new(TestSystem::new(os_system));

            match Server::new(worker_threads, server_connection, test_system, true) {
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

        let workspace_folders = workspaces
            .iter()
            .map(|(folder, _)| folder.clone())
            .collect::<Vec<_>>();

        let workspace_configurations = workspaces
            .into_iter()
            .filter_map(|(folder, options)| Some((folder.uri, options?)))
            .collect::<HashMap<_, _>>();

        Self {
            server_thread: Some(server_thread),
            client_connection: Some(client_connection),
            test_context,
            request_counter: 0,
            responses: FxHashMap::default(),
            notifications: VecDeque::new(),
            requests: VecDeque::new(),
            initialize_response: None,
            workspace_configurations,
            shutdown_requested: false,
        }
        .initialize(workspace_folders, capabilities, initialization_options)
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
        initialization_options: Option<ClientOptions>,
    ) -> Result<Self> {
        let init_params = InitializeParams {
            capabilities,
            workspace_folders: Some(workspace_folders),
            initialization_options: initialization_options.map(|options| {
                serde_json::to_value(options)
                    .context("Failed to serialize initialization options to `ClientOptions`")
                    .unwrap()
            }),
            ..Default::default()
        };

        let init_request_id = self.send_request::<Initialize>(init_params);
        self.initialize_response = Some(self.await_response::<Initialize>(&init_request_id)?);
        self.send_notification::<Initialized>(InitializedParams {});

        Ok(self)
    }

    /// Wait until the server has initialized all workspaces.
    ///
    /// This will wait until the client receives a `workspace/configuration` request from the
    /// server, and handles the request.
    ///
    /// This should only be called if the server is expected to send this request.
    pub(crate) fn wait_until_workspaces_are_initialized(mut self) -> Result<Self> {
        let (request_id, params) = self.await_request::<WorkspaceConfiguration>()?;
        self.handle_workspace_configuration_request(request_id, &params)?;
        Ok(self)
    }

    /// Drain all messages from the server.
    fn drain_messages(&mut self) {
        loop {
            // Don't wait too long to drain the messages, as this is called in the `Drop`
            // implementation which happens everytime the test ends.
            match self.receive(Some(Duration::from_millis(10))) {
                Ok(()) => {}
                Err(TestServerError::RecvTimeoutError(_)) => {
                    // Only break if we have no more messages to process.
                    break;
                }
                Err(err) => {
                    tracing::error!("Error while draining messages: {err:?}");
                }
            }
        }
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
    /// [`send_request`]: TestServer::send_request
    pub(crate) fn await_response<R>(&mut self, id: &RequestId) -> Result<R::Result>
    where
        R: Request,
    {
        loop {
            if let Some(response) = self.responses.remove(id) {
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
                        return Err(TestServerError::ResponseError(err).into());
                    }
                    response => {
                        return Err(TestServerError::InvalidResponse(
                            id.clone(),
                            Box::new(response),
                        )
                        .into());
                    }
                }
            }

            self.receive_or_panic()?;
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
    pub(crate) fn await_notification<N: Notification>(&mut self) -> Result<N::Params> {
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
                return Ok(serde_json::from_value(notification.params)?);
            }
            self.receive_or_panic()?;
        }
        Err(anyhow::anyhow!(
            "Failed to receive `{}` notification after {RETRY_COUNT} retries",
            N::METHOD
        ))
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
    pub(crate) fn await_request<R: Request>(&mut self) -> Result<(RequestId, R::Params)> {
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
            self.receive_or_panic()?;
        }
        Err(anyhow::anyhow!(
            "Failed to receive `{}` request after {RETRY_COUNT} retries",
            R::METHOD
        ))
    }

    /// Receive a message from the server.
    ///
    /// It will wait for `timeout` duration for a message to arrive. If no message is received
    /// within that time, it will return an error.
    ///
    /// If `timeout` is `None`, it will use a default timeout of 1 second.
    fn receive(&mut self, timeout: Option<Duration>) -> Result<(), TestServerError> {
        static DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

        let receiver = self.client_connection.as_ref().unwrap().receiver.clone();
        let message = receiver
            .recv_timeout(timeout.unwrap_or(DEFAULT_TIMEOUT))
            .map_err(TestServerError::RecvTimeoutError)?;

        self.handle_message(message)?;

        for message in receiver.try_iter() {
            self.handle_message(message)?;
        }

        Ok(())
    }

    /// This is a convenience method that's same as [`receive`], but panics if the server got
    /// disconnected. It will pass other errors as is.
    ///
    /// [`receive`]: TestServer::receive
    fn receive_or_panic(&mut self) -> Result<(), TestServerError> {
        if let Err(err) = self.receive(None) {
            if err.is_disconnected() {
                self.panic_on_server_disconnect();
            } else {
                return Err(err);
            }
        }
        Ok(())
    }

    /// Handle the incoming message from the server.
    ///
    /// This method will store the message as follows:
    /// - Requests are stored in `self.requests`
    /// - Responses are stored in `self.responses` with the request ID as the key
    /// - Notifications are stored in `self.notifications`
    fn handle_message(&mut self, message: Message) -> Result<(), TestServerError> {
        match message {
            Message::Request(request) => {
                tracing::debug!("Received server request {}", &request.method);
                self.requests.push_back(request);
            }
            Message::Response(response) => {
                tracing::debug!("Received server response for request {}", &response.id);
                match self.responses.entry(response.id.clone()) {
                    Entry::Occupied(existing) => {
                        return Err(TestServerError::DuplicateResponse(
                            response.id,
                            Box::new(existing.get().clone()),
                        ));
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(response);
                    }
                }
            }
            Message::Notification(notification) => {
                tracing::debug!("Received notification {}", &notification.method);
                self.notifications.push_back(notification);
            }
        }
        Ok(())
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

    pub(crate) fn cancel(&mut self, request_id: &RequestId) {
        let id_string = request_id.to_string();
        self.send_notification::<lsp_types::notification::Cancel>(lsp_types::CancelParams {
            id: match id_string.parse() {
                Ok(id) => NumberOrString::Number(id),
                Err(_) => NumberOrString::String(id_string),
            },
        });
    }

    /// Handle workspace configuration requests from the server.
    ///
    /// Use the [`get_request`] method to wait for the server to send this request.
    ///
    /// [`get_request`]: TestServer::get_request
    fn handle_workspace_configuration_request(
        &mut self,
        request_id: RequestId,
        params: &ConfigurationParams,
    ) -> Result<()> {
        let mut results = Vec::new();

        for item in &params.items {
            let Some(scope_uri) = &item.scope_uri else {
                unimplemented!("Handling global configuration requests is not implemented yet");
            };
            let config_value = if let Some(options) = self.workspace_configurations.get(scope_uri) {
                // Return the configuration for the specific workspace
                //
                // As per the spec:
                //
                // > If the client can't provide a configuration setting for a given scope
                // > then null needs to be present in the returned array.
                match item.section.as_deref() {
                    Some("ty") => serde_json::to_value(options)?,
                    Some(section) => {
                        tracing::debug!("Unrecognized section `{section}` for {scope_uri}");
                        serde_json::Value::Null
                    }
                    None => {
                        tracing::debug!(
                            "No section specified for workspace configuration of {scope_uri}",
                        );
                        serde_json::Value::Null
                    }
                }
            } else {
                tracing::debug!("No workspace configuration provided for {scope_uri}");
                serde_json::Value::Null
            };
            results.push(config_value);
        }

        let response = Response::new_ok(request_id, results);
        self.send(Message::Response(response));

        Ok(())
    }

    /// Get the initialization result
    pub(crate) fn initialization_result(&self) -> Option<&InitializeResult> {
        self.initialize_response.as_ref()
    }

    fn file_uri(&self, path: impl AsRef<SystemPath>) -> Url {
        Url::from_file_path(self.test_context.root().join(path.as_ref()).as_std_path())
            .expect("Path must be a valid URL")
    }

    /// Send a `textDocument/didOpen` notification
    pub(crate) fn open_text_document(
        &mut self,
        path: impl AsRef<SystemPath>,
        content: &impl ToString,
        version: i32,
    ) {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: self.file_uri(path),
                language_id: "python".to_string(),
                version,
                text: content.to_string(),
            },
        };
        self.send_notification::<DidOpenTextDocument>(params);
    }

    /// Send a `textDocument/didChange` notification with the given content changes
    pub(crate) fn change_text_document(
        &mut self,
        path: impl AsRef<SystemPath>,
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
    pub(crate) fn close_text_document(&mut self, path: impl AsRef<SystemPath>) {
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

    /// Send a `textDocument/diagnostic` request for the document at the given path.
    pub(crate) fn document_diagnostic_request(
        &mut self,
        path: impl AsRef<SystemPath>,
        previous_result_id: Option<String>,
    ) -> Result<DocumentDiagnosticReportResult> {
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

    /// Send a `workspace/diagnostic` request with optional previous result IDs.
    pub(crate) fn workspace_diagnostic_request(
        &mut self,
        work_done_token: Option<lsp_types::NumberOrString>,
        previous_result_ids: Option<Vec<PreviousResultId>>,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        let params = WorkspaceDiagnosticParams {
            identifier: Some("ty".to_string()),
            previous_result_ids: previous_result_ids.unwrap_or_default(),
            work_done_progress_params: WorkDoneProgressParams { work_done_token },
            partial_result_params: PartialResultParams::default(),
        };

        let id = self.send_request::<WorkspaceDiagnosticRequest>(params);
        self.await_response::<WorkspaceDiagnosticRequest>(&id)
    }

    /// Send a `textDocument/hover` request for the document at the given path and position.
    pub(crate) fn hover_request(
        &mut self,
        path: impl AsRef<SystemPath>,
        position: Position,
    ) -> Result<Option<Hover>> {
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

    /// Sends a `textDocument/inlayHint` request for the document at the given path and range.
    pub(crate) fn inlay_hints_request(
        &mut self,
        path: impl AsRef<SystemPath>,
        range: Range,
    ) -> Result<Option<Vec<InlayHint>>> {
        let params = InlayHintParams {
            text_document: TextDocumentIdentifier {
                uri: self.file_uri(path),
            },
            range,
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let id = self.send_request::<InlayHintRequest>(params);
        self.await_response::<InlayHintRequest>(&id)
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
            .field("workspace_configurations", &self.workspace_configurations)
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
            match self.await_response::<Shutdown>(&shutdown_id) {
                Ok(()) => {
                    self.send_notification::<Exit>(());
                    None
                }
                Err(err) => Some(format!("Failed to get shutdown response: {err:?}")),
            }
        } else {
            None
        };

        if let Some(_client_connection) = self.client_connection.take() {
            // Drop the client connection before joining the server thread to avoid any hangs
            // in case the server didn't respond to the shutdown request.
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
    workspaces: Vec<(WorkspaceFolder, Option<ClientOptions>)>,
    initialization_options: Option<ClientOptions>,
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
        let client_capabilities = ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
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
    pub(crate) fn with_initialization_options(mut self, options: ClientOptions) -> Self {
        self.initialization_options = Some(options);
        self
    }

    /// Add a workspace to the test server with the given root path and options.
    ///
    /// This option will be used to respond to the `workspace/configuration` request that the
    /// server will send to the client.
    ///
    /// If `options` is `None`, the test server will respond with `null` for this workspace
    /// when the server sends a `workspace/configuration` request.
    pub(crate) fn with_workspace(
        mut self,
        workspace_root: &SystemPath,
        options: Option<ClientOptions>,
    ) -> Result<Self> {
        // TODO: Support multiple workspaces in the test server
        if self.workspaces.len() == 1 {
            anyhow::bail!("Test server doesn't support multiple workspaces yet");
        }

        let workspace_path = self.test_context.root().join(workspace_root);
        fs::create_dir_all(workspace_path.as_std_path())?;

        self.workspaces.push((
            WorkspaceFolder {
                uri: Url::from_file_path(workspace_path.as_std_path()).map_err(|()| {
                    anyhow!("Failed to convert workspace path to URL: {workspace_path}")
                })?,
                name: workspace_root.file_name().unwrap_or("test").to_string(),
            },
            options,
        ));

        Ok(self)
    }

    /// Enable or disable pull diagnostics capability
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
    pub(crate) fn enable_diagnostic_dynamic_registration(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .text_document
            .get_or_insert_default()
            .diagnostic
            .get_or_insert_default()
            .dynamic_registration = Some(enabled);
        self
    }

    /// Enable or disable dynamic registration of rename capability
    pub(crate) fn enable_rename_dynamic_registration(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .text_document
            .get_or_insert_default()
            .rename
            .get_or_insert_default()
            .dynamic_registration = Some(enabled);
        self
    }

    /// Enable or disable workspace configuration capability
    pub(crate) fn enable_workspace_configuration(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .workspace
            .get_or_insert_default()
            .configuration = Some(enabled);
        self
    }

    /// Enable or disable inlay hints capability
    pub(crate) fn enable_inlay_hints(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .text_document
            .get_or_insert_default()
            .inlay_hint = if enabled {
            Some(InlayHintClientCapabilities::default())
        } else {
            None
        };
        self
    }

    /// Enable or disable file watching capability
    #[expect(dead_code)]
    pub(crate) fn enable_did_change_watched_files(mut self, enabled: bool) -> Self {
        self.client_capabilities
            .workspace
            .get_or_insert_default()
            .did_change_watched_files = if enabled {
            Some(DidChangeWatchedFilesClientCapabilities::default())
        } else {
            None
        };
        self
    }

    /// Set custom client capabilities (overrides any previously set capabilities)
    #[expect(dead_code)]
    pub(crate) fn with_client_capabilities(mut self, capabilities: ClientCapabilities) -> Self {
        self.client_capabilities = capabilities;
        self
    }

    /// Write a file to the test directory
    pub(crate) fn with_file(
        self,
        path: impl AsRef<SystemPath>,
        content: impl AsRef<str>,
    ) -> Result<Self> {
        let file_path = self.test_context.root().join(path.as_ref());
        // Ensure parent directories exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent.as_std_path())?;
        }
        fs::write(file_path.as_std_path(), content.as_ref())?;
        Ok(self)
    }

    /// Write multiple files to the test directory
    #[expect(dead_code)]
    pub(crate) fn with_files<P, C, I>(mut self, files: I) -> Result<Self>
    where
        I: IntoIterator<Item = (P, C)>,
        P: AsRef<SystemPath>,
        C: AsRef<str>,
    {
        for (path, content) in files {
            self = self.with_file(path, content)?;
        }
        Ok(self)
    }

    /// Build the test server
    pub(crate) fn build(self) -> Result<TestServer> {
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
    project_dir: SystemPathBuf,
}

impl TestContext {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macos uses symlinks for tempdirs
        // and that doesn't play well with our snapshot filtering.
        // Simplify with dunce because otherwise we get UNC paths on Windows.
        let project_dir = SystemPathBuf::from_path_buf(
            dunce::simplified(
                &temp_dir
                    .path()
                    .canonicalize()
                    .context("Failed to canonicalize project path")?,
            )
            .to_path_buf(),
        )
        .map_err(|path| {
            anyhow!(
                "Failed to create test directory: `{}` contains non-Unicode characters",
                path.display()
            )
        })?;

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&tempdir_filter(project_dir.as_str()), "<temp_dir>/");
        settings.add_filter(
            &tempdir_filter(
                Url::from_file_path(project_dir.as_std_path())
                    .map_err(|()| anyhow!("Failed to convert root directory to url"))?
                    .path(),
            ),
            "<temp_dir>/",
        );
        settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");
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

    pub(crate) fn root(&self) -> &SystemPath {
        &self.project_dir
    }
}

fn tempdir_filter(path: impl AsRef<str>) -> String {
    format!(r"{}\\?/?", regex::escape(path.as_ref()))
}
