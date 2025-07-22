//! Testing server for the ty language server.
//!
//! This module provides mock server infrastructure for testing LSP functionality using
//! temporary directories on the real filesystem.
//!
//! The design is inspired by the Starlark LSP test server but adapted for ty server architecture.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{fmt, fs};

use anyhow::Result;
use crossbeam::channel::RecvTimeoutError;
use lsp_server::{Connection, Message, RequestId, Response, ResponseError};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument, Exit,
    Initialized, Notification,
};
use lsp_types::request::{
    DocumentDiagnosticRequest, Initialize, Request, Shutdown, WorkspaceConfiguration,
};
use lsp_types::{
    ClientCapabilities, ConfigurationParams, DiagnosticClientCapabilities,
    DidChangeTextDocumentParams, DidChangeWatchedFilesClientCapabilities,
    DidChangeWatchedFilesParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentDiagnosticParams, DocumentDiagnosticReportResult, FileEvent, InitializeParams,
    InitializeResult, InitializedParams, PartialResultParams, PublishDiagnosticsClientCapabilities,
    TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, Url, VersionedTextDocumentIdentifier, WorkDoneProgressParams,
    WorkspaceClientCapabilities, WorkspaceFolder,
};
use ruff_db::system::{OsSystem, SystemPath, TestSystem};
use serde::de::DeserializeOwned;
use tempfile::TempDir;

use crate::logging::{LogLevel, init_logging};
use crate::server::Server;
use crate::session::ClientOptions;

/// Number of times to retry receiving a message before giving up
const RETRY_COUNT: usize = 5;

static INIT_TRACING: OnceLock<()> = OnceLock::new();

/// Setup tracing for the test server.
///
/// This will make sure that the tracing subscriber is initialized only once, so that running
/// multiple tests does not cause multiple subscribers to be registered.
fn setup_tracing() {
    INIT_TRACING.get_or_init(|| {
        init_logging(LogLevel::Info, None);
    });
}

/// Errors that can occur during testing
#[derive(thiserror::Error, Debug)]
pub(crate) enum TestServerError {
    /// The response came back, but was an error response, not a successful one.
    #[error("Response error: {0:?}")]
    ResponseError(ResponseError),

    #[error("Invalid response message for request {0}: {1:?}")]
    InvalidResponse(RequestId, Response),

    #[error("Got a duplicate response for request ID {0}: {1:?}")]
    DuplicateResponse(RequestId, Response),

    #[error("Timeout while waiting for a message from the server")]
    RecvTimeoutError,
}

/// A test server for the ty language server that provides helpers for sending requests,
/// correlating responses, and handling notifications.
///
/// The [`Drop`] implementation ensures that the server is shut down gracefully using the described
/// protocol in the LSP specification. It also ensures that all messages sent by the server have
/// been handled by the test client before the server is dropped.
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

    /// Temporary directory that holds all test files.
    ///
    /// This directory is automatically cleaned up when the [`TestServer`] is dropped.
    temp_dir: TempDir,

    /// Incrementing counter to automatically generate request IDs
    request_counter: i32,

    /// Simple incrementing document version counter
    version_counter: i32,

    /// A mapping of request IDs to responses received from the server
    responses: HashMap<RequestId, Response>,

    /// An ordered queue of all the notifications received from the server
    notifications: VecDeque<lsp_server::Notification>,

    /// An ordered queue of all the requests received from the server
    requests: VecDeque<lsp_server::Request>,

    /// The response from server initialization
    initialize_response: Option<InitializeResult>,

    /// Workspace configurations for `workspace/configuration` requests
    workspace_configurations: HashMap<Url, ClientOptions>,

    /// Capabilities registered by the server
    registered_capabilities: Vec<String>,
}

impl TestServer {
    /// Create a new test server with the given workspace configurations
    pub(crate) fn new(
        workspaces: Vec<(WorkspaceFolder, ClientOptions)>,
        temp_dir: TempDir,
        capabilities: ClientCapabilities,
    ) -> Result<Self> {
        setup_tracing();

        let (server_connection, client_connection) = Connection::memory();

        // Create OS system with the temp directory as cwd
        let temp_path = SystemPath::from_std_path(temp_dir.path()).unwrap();
        let os_system = OsSystem::new(temp_path);

        // Start the server in a separate thread
        let server_thread = std::thread::spawn(move || {
            // TODO: This should probably be configurable to test concurrency issues
            let worker_threads = NonZeroUsize::new(1).unwrap();
            let test_system = Arc::new(TestSystem::new(os_system));

            match Server::new(worker_threads, server_connection, test_system, false) {
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
            .map(|(folder, options)| (folder.uri, options))
            .collect::<HashMap<_, _>>();

        Self {
            server_thread: Some(server_thread),
            client_connection: Some(client_connection),
            temp_dir,
            request_counter: 0,
            version_counter: 0,
            responses: HashMap::new(),
            notifications: VecDeque::new(),
            requests: VecDeque::new(),
            initialize_response: None,
            workspace_configurations,
            registered_capabilities: Vec::new(),
        }
        .initialize(workspace_folders, capabilities)
    }

    /// Perform LSP initialization handshake
    fn initialize(
        mut self,
        workspace_folders: Vec<WorkspaceFolder>,
        capabilities: ClientCapabilities,
    ) -> Result<Self> {
        let init_params = InitializeParams {
            capabilities,
            workspace_folders: Some(workspace_folders),
            // TODO: This should be configurable by the test server builder. This might not be
            // required after client settings are implemented in the server.
            initialization_options: Some(serde_json::Value::Object(serde_json::Map::new())),
            ..Default::default()
        };

        let init_request_id = self.send_request::<Initialize>(init_params);
        self.initialize_response = Some(self.await_response::<InitializeResult>(init_request_id)?);
        self.send_notification::<Initialized>(InitializedParams {});

        Ok(self)
    }

    pub(crate) fn temp_dir(&self) -> &TempDir {
        &self.temp_dir
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
                Err(TestServerError::RecvTimeoutError) => {
                    // Only break if we have no more messages to process.
                    break;
                }
                Err(_) => {}
            }
        }
    }

    /// Validate that there are no pending messages from the server.
    ///
    /// This should be called before the test server is dropped to ensure that all server messages
    /// have been properly consumed by the test. If there are any pending messages, this will panic
    /// with detailed information about what was left unconsumed.
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

    /// Generate a new document version
    fn next_document_version(&mut self) -> i32 {
        self.version_counter += 1;
        self.version_counter
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
    /// The caller can use this ID to later retrieve the response using [`get_response`].
    ///
    /// [`get_response`]: TestServer::get_response
    pub(crate) fn send_request<R>(&mut self, params: R::Params) -> RequestId
    where
        R: Request,
    {
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
    pub(crate) fn await_response<T: DeserializeOwned>(&mut self, id: RequestId) -> Result<T> {
        loop {
            self.receive(None)?;

            if let Some(response) = self.responses.remove(&id) {
                match response {
                    Response {
                        error: None,
                        result: Some(result),
                        ..
                    } => {
                        return Ok(serde_json::from_value::<T>(result)?);
                    }
                    Response {
                        error: Some(err),
                        result: None,
                        ..
                    } => {
                        return Err(TestServerError::ResponseError(err).into());
                    }
                    response => {
                        return Err(TestServerError::InvalidResponse(id, response).into());
                    }
                }
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
    pub(crate) fn await_notification<N: Notification>(&mut self) -> Result<N::Params> {
        for _ in 0..RETRY_COUNT {
            self.receive(None)?;
            let notification = self
                .notifications
                .iter()
                .position(|notification| N::METHOD == notification.method)
                .and_then(|index| self.notifications.remove(index));
            if let Some(notification) = notification {
                return Ok(serde_json::from_value(notification.params)?);
            }
            tracing::info!("Retrying to receive `{}` notification", N::METHOD);
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
        for _ in 0..RETRY_COUNT {
            self.receive(None)?;
            let request = self
                .requests
                .iter()
                .position(|request| R::METHOD == request.method)
                .and_then(|index| self.requests.remove(index));
            if let Some(request) = request {
                let params = serde_json::from_value(request.params)?;
                return Ok((request.id, params));
            }
            tracing::info!("Retrying to receive `{}` request", R::METHOD);
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
    #[allow(clippy::result_large_err)]
    fn receive(&mut self, timeout: Option<Duration>) -> Result<(), TestServerError> {
        static DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

        match self
            .client_connection
            .as_ref()
            .unwrap()
            .receiver
            .recv_timeout(timeout.unwrap_or(DEFAULT_TIMEOUT))
        {
            Ok(message) => self.handle_message(message),
            Err(RecvTimeoutError::Timeout) => Err(TestServerError::RecvTimeoutError),
            Err(RecvTimeoutError::Disconnected) => {
                self.panic_on_server_disconnect();
            }
        }
    }

    /// Handle the incoming message from the server.
    ///
    /// This method will store the message as follows:
    /// - Requests are stored in `self.requests`
    /// - Responses are stored in `self.responses` with the request ID as the key
    /// - Notifications are stored in `self.notifications`
    #[allow(clippy::result_large_err)]
    fn handle_message(&mut self, message: Message) -> Result<(), TestServerError> {
        match message {
            Message::Request(request) => {
                self.requests.push_back(request);
            }
            Message::Response(response) => match self.responses.entry(response.id.clone()) {
                Entry::Occupied(existing) => {
                    return Err(TestServerError::DuplicateResponse(
                        response.id,
                        existing.get().clone(),
                    ));
                }
                Entry::Vacant(entry) => {
                    entry.insert(response);
                }
            },
            Message::Notification(notification) => {
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

    /// Handle workspace configuration requests from the server.
    ///
    /// Use the [`get_request`] method to wait for the server to send this request.
    ///
    /// [`get_request`]: TestServer::get_request
    pub(crate) fn handle_workspace_configuration_request(
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
                match item.section.as_deref() {
                    Some("ty") => serde_json::to_value(options)?,
                    Some(_) | None => {
                        // TODO: Handle `python` section once it's implemented in the server
                        // As per the spec:
                        //
                        // > If the client can't provide a configuration setting for a given scope
                        // > then null needs to be present in the returned array.
                        serde_json::Value::Null
                    }
                }
            } else {
                tracing::warn!("No workspace configuration found for {scope_uri}");
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
        let temp_dir = SystemPath::from_std_path(self.temp_dir.path()).unwrap();
        Url::from_file_path(temp_dir.join(path.as_ref()).as_std_path())
            .expect("Path must be a valid URL")
    }

    /// Send a `textDocument/didOpen` notification
    pub(crate) fn open_text_document(
        &mut self,
        path: impl AsRef<SystemPath>,
        content: &impl ToString,
    ) {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: self.file_uri(path),
                language_id: "python".to_string(),
                version: self.next_document_version(),
                text: content.to_string(),
            },
        };
        self.send_notification::<DidOpenTextDocument>(params);
    }

    /// Send a `textDocument/didChange` notification with the given content changes
    #[expect(dead_code)]
    pub(crate) fn change_text_document(
        &mut self,
        path: impl AsRef<SystemPath>,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) {
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: self.file_uri(path),
                version: self.next_document_version(),
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
    ) -> Result<DocumentDiagnosticReportResult> {
        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier {
                uri: self.file_uri(path),
            },
            identifier: Some("ty".to_string()),
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let id = self.send_request::<DocumentDiagnosticRequest>(params);
        self.await_response::<DocumentDiagnosticReportResult>(id)
    }
}

impl fmt::Debug for TestServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TestServer")
            .field("temp_dir", &self.temp_dir.path())
            .field("request_counter", &self.request_counter)
            .field("version_counter", &self.version_counter)
            .field("responses", &self.responses)
            .field("notifications", &self.notifications)
            .field("server_requests", &self.requests)
            .field("initialize_response", &self.initialize_response)
            .field("workspace_configurations", &self.workspace_configurations)
            .field("registered_capabilities", &self.registered_capabilities)
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
        let shutdown_error = self
            .server_thread
            .is_some()
            .then(|| {
                let shutdown_id = self.send_request::<Shutdown>(());
                match self.await_response::<()>(shutdown_id) {
                    Ok(()) => {
                        self.send_notification::<Exit>(());
                        None
                    }
                    Err(err) => Some(format!("Failed to get shutdown response: {err:?}")),
                }
            })
            .flatten();

        if let Some(_client_connection) = self.client_connection.take() {
            // Drop the client connection before joining the server thread to avoid any hangs
            // in case the server didn't respond to the shutdown request.
        }

        if let Some(server_thread) = self.server_thread.take() {
            if let Err(err) = server_thread.join() {
                panic!("Panic in the server thread: {err:?}");
            }
        }

        if std::thread::panicking() {
            // If the test server panicked, avoid further assertions.
            return;
        }

        if let Some(error) = shutdown_error {
            panic!("Test server did not shut down gracefully: {error}");
        }

        self.assert_no_pending_messages();
    }
}

/// Builder for creating test servers with specific configurations
pub(crate) struct TestServerBuilder {
    temp_dir: TempDir,
    workspaces: Vec<(WorkspaceFolder, ClientOptions)>,
    client_capabilities: ClientCapabilities,
}

impl TestServerBuilder {
    /// Create a new builder
    pub(crate) fn new() -> Self {
        // Default client capabilities for the test server. These are assumptions made by the real
        // server and are common for most clients:
        //
        // - Supports publishing diagnostics
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

        Self {
            workspaces: Vec::new(),
            temp_dir: TempDir::new().expect("should be able to create temporary directory"),
            client_capabilities,
        }
    }

    /// Add a workspace configuration
    pub(crate) fn with_workspace(
        mut self,
        workspace_root: &SystemPath,
        options: ClientOptions,
    ) -> Result<Self> {
        let temp_system_path = SystemPath::from_std_path(self.temp_dir.path()).unwrap();
        let workspace_path = temp_system_path.join(workspace_root);

        fs::create_dir_all(workspace_path.as_std_path())?;

        self.workspaces.push((
            WorkspaceFolder {
                uri: Url::from_file_path(workspace_path.as_std_path())
                    .expect("workspace root should be a valid URL"),
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
            .get_or_insert_with(Default::default)
            .diagnostic = if enabled {
            Some(DiagnosticClientCapabilities::default())
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
            .get_or_insert_with(Default::default)
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

    /// Write a file to the temporary directory
    pub(crate) fn write_file(
        self,
        path: impl AsRef<SystemPath>,
        content: impl AsRef<str>,
    ) -> Result<Self> {
        let temp_path = SystemPath::from_std_path(self.temp_dir.path()).unwrap();
        let file_path = temp_path.join(path.as_ref());

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent.as_std_path())?;
        }

        fs::write(file_path.as_std_path(), content.as_ref())?;
        Ok(self)
    }

    /// Write multiple files to the temporary directory
    #[expect(dead_code)]
    pub(crate) fn write_files<P, C, I>(mut self, files: I) -> Result<Self>
    where
        I: IntoIterator<Item = (P, C)>,
        P: AsRef<SystemPath>,
        C: AsRef<str>,
    {
        for (path, content) in files {
            self = self.write_file(path, content)?;
        }
        Ok(self)
    }

    /// Build the test server
    pub(crate) fn build(self) -> Result<TestServer> {
        TestServer::new(self.workspaces, self.temp_dir, self.client_capabilities)
    }
}
