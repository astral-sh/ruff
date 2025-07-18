//! Testing server for the ty language server.
//!
//! This module provides mock server infrastructure for testing LSP functionality without requiring
//! actual file system operations.
//!
//! The design is inspired by the Starlark LSP test server but adapted for ty server architecture.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

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
    RegistrationParams, TextDocumentClientCapabilities, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams, WorkspaceClientCapabilities, WorkspaceFolder,
};
use ruff_db::system::{InMemorySystem, SystemPath, TestSystem};
use serde::de::DeserializeOwned;

use crate::server::Server;
use crate::session::ClientOptions;

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

    #[error("Test client received an unrecognized request from the server: {0:?}")]
    UnrecognizedRequest(lsp_server::Request),

    #[error(transparent)]
    RecvTimeoutError(#[from] RecvTimeoutError),
}

/// A test server for the ty language server that provides helpers for sending requests,
/// correlating responses, and handling notifications.
///
/// The [`Drop`] implementation ensures that the server is shut down gracefully using the described
/// protocol in the LSP specification.
pub(crate) struct TestServer {
    /// The thread that's actually running the server
    server_thread: Option<JoinHandle<()>>,

    /// Connection to communicate with the server
    client_connection: Connection,

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

    /// How long to wait for messages to be received
    recv_timeout: Duration,

    /// The response from server initialization
    initialize_response: Option<InitializeResult>,

    /// Workspace configurations for `workspace/configuration` requests
    workspace_configurations: HashMap<Url, ClientOptions>,

    /// Capabilities registered by the server
    registered_capabilities: Vec<String>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.drain_messages();

        // Follow the LSP protocol to shutdown the server gracefully
        let shutdown_error = match self.send_request::<Shutdown>(()) {
            Ok(shutdown_id) => match self.get_response::<()>(shutdown_id) {
                Ok(()) => {
                    if let Err(err) = self.send_notification::<Exit>(()) {
                        Some(format!("Failed to send exit notification: {err:?}"))
                    } else {
                        None
                    }
                }
                Err(err) => Some(format!("Failed to get shutdown response: {err:?}")),
            },
            Err(err) => Some(format!("Failed to send shutdown request: {err:?}")),
        };

        if let Some(server_thread) = self.server_thread.take() {
            if let Err(err) = server_thread.join() {
                panic!("Test server thread did not join when dropped: {err:?}");
            }
        }

        if let Some(error) = shutdown_error {
            panic!("Test server did not shut down gracefully: {error}");
        }

        self.assert_no_pending_messages();
    }
}

impl fmt::Debug for TestServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TestServer")
            .field("request_counter", &self.request_counter)
            .field("version_counter", &self.version_counter)
            .field("responses", &self.responses)
            .field("notifications", &self.notifications)
            .field("server_requests", &self.requests)
            .field("recv_timeout", &self.recv_timeout)
            .field("initialize_response", &self.initialize_response)
            .field("workspace_configurations", &self.workspace_configurations)
            .field("registered_capabilities", &self.registered_capabilities)
            .finish_non_exhaustive()
    }
}

impl TestServer {
    /// Create a new test server with the given workspace configurations
    pub(crate) fn new(
        workspace_folders: Vec<WorkspaceFolder>,
        workspace_configurations: HashMap<Url, ClientOptions>,
        memory_system: InMemorySystem,
        capabilities: ClientCapabilities,
    ) -> Result<Self> {
        assert_eq!(
            workspace_folders.len(),
            workspace_configurations.len(),
            "Number of workspace folders should match the number of workspace configurations"
        );

        let (server_connection, client_connection) = Connection::memory();

        // Start the server in a separate thread
        let server_thread = std::thread::spawn(move || {
            // TODO: This should probably be configurable to test concurrency issues
            let worker_threads = NonZeroUsize::new(1).unwrap();
            let test_system = Arc::new(TestSystem::new(memory_system));

            match Server::new(worker_threads, server_connection, test_system) {
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
            client_connection,
            request_counter: 0,
            version_counter: 0,
            responses: HashMap::new(),
            notifications: VecDeque::new(),
            requests: VecDeque::new(),
            recv_timeout: Duration::from_secs(2),
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

        let init_request_id = self.send_request::<Initialize>(init_params)?;
        self.initialize_response = Some(self.get_response::<InitializeResult>(init_request_id)?);
        self.send_notification::<Initialized>(InitializedParams {})?;

        Ok(self)
    }

    /// Wait until the server has initialized all workspaces.
    ///
    /// This will wait until the client receives a `workspace/configuration` request from the
    /// server, and handles the request.
    ///
    /// This should only be called if the server is expected to send this request.
    pub(crate) fn wait_until_workspaces_are_initialized(mut self) -> Result<Self> {
        let (request_id, params) = self.get_request::<WorkspaceConfiguration>()?;
        self.handle_workspace_configuration_request(request_id, &params)?;
        Ok(self)
    }

    /// Drain all messages from the server.
    fn drain_messages(&mut self) {
        loop {
            match self.receive() {
                Ok(()) => {}
                Err(TestServerError::RecvTimeoutError(_)) => {
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

    /// Send a request to the server and return the request ID.
    ///
    /// The caller can use this ID to later retrieve the response using [`get_response`].
    ///
    /// [`get_response`]: TestServer::get_response
    pub(crate) fn send_request<R>(&mut self, params: R::Params) -> Result<RequestId>
    where
        R: Request,
    {
        let id = self.next_request_id();
        let request = lsp_server::Request::new(id.clone(), R::METHOD.to_string(), params);
        self.client_connection
            .sender
            .send(Message::Request(request))?;
        Ok(id)
    }

    /// Send a notification to the server.
    pub(crate) fn send_notification<N>(&self, params: N::Params) -> Result<()>
    where
        N: Notification,
    {
        let notification = lsp_server::Notification::new(N::METHOD.to_string(), params);
        self.client_connection
            .sender
            .send(Message::Notification(notification))?;
        Ok(())
    }

    /// Get a server response for the given request ID.
    ///
    /// This should only be called if a request was already sent to the server via [`send_request`]
    /// which returns the request ID that should be used here.
    ///
    /// This method will remove the response from the internal data structure, so it can only be
    /// called once per request ID.
    ///
    /// [`send_request`]: TestServer::send_request
    pub(crate) fn get_response<T: DeserializeOwned>(&mut self, id: RequestId) -> Result<T> {
        loop {
            self.receive()?;

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

    /// Get a notification of the specified type from the server and return its parameters.
    ///
    /// The caller should ensure that the server is expected to send this notification type. It
    /// will keep polling the server for this notification up to 10 times before giving up after
    /// which it will return an error. It will also return an error if the notification is not
    /// received within `recv_timeout` duration.
    ///
    /// This method will remove the notification from the internal data structure, so it should
    /// only be called if the notification is expected to be sent by the server.
    pub(crate) fn get_notification<N: Notification>(&mut self) -> Result<N::Params> {
        for _ in 0..10 {
            self.receive()?;
            let notification = self
                .notifications
                .iter()
                .enumerate()
                .find_map(|(index, notification)| {
                    if N::METHOD == notification.method {
                        Some(index)
                    } else {
                        None
                    }
                })
                .and_then(|index| self.notifications.remove(index));
            if let Some(notification) = notification {
                return Ok(serde_json::from_value(notification.params)?);
            }
        }
        Err(anyhow::anyhow!(
            "Did not get a notification of type `{}` in 10 retries",
            N::METHOD
        ))
    }

    /// Get a request of the specified type from the server and return the request ID and
    /// parameters.
    ///
    /// The caller should ensure that the server is expected to send this request type. It will
    /// keep polling the server for this request up to 10 times before giving up after which it
    /// will return an error. It can also return an error if the request is not received within
    /// `recv_timeout` duration.
    ///
    /// This method will remove the request from the internal data structure, so it should only be
    /// called if the request is expected to be sent by the server.
    pub(crate) fn get_request<R: Request>(&mut self) -> Result<(RequestId, R::Params)> {
        for _ in 0..10 {
            self.receive()?;
            let request = self
                .requests
                .iter()
                .enumerate()
                .find_map(|(index, request)| {
                    if R::METHOD == request.method {
                        Some(index)
                    } else {
                        None
                    }
                })
                .and_then(|index| self.requests.remove(index));
            if let Some(request) = request {
                let params = serde_json::from_value(request.params)?;
                return Ok((request.id, params));
            }
        }
        Err(anyhow::anyhow!(
            "Did not get a request of type `{}` in 10 retries",
            R::METHOD
        ))
    }

    /// Receive a message from the server.
    ///
    /// It will wait for `recv_timeout` duration for a message to arrive. If no message is received
    /// within that time, it will return an error.
    ///
    /// Once a message is received, it will store it in the appropriate queue:
    /// - Requests are stored in `requests`
    /// - Responses are stored in `responses`
    /// - Notifications are stored in `notifications`
    #[allow(clippy::result_large_err)]
    fn receive(&mut self) -> Result<(), TestServerError> {
        let message = self
            .client_connection
            .receiver
            .recv_timeout(self.recv_timeout)?;

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
                // TODO: Should we return an error? User should explicitly configure the test
                // server to have a configuration for all workspaces even if they are empty.
                serde_json::Value::Null
            };
            results.push(config_value);
        }

        let response = Response::new_ok(request_id, results);
        self.client_connection
            .sender
            .send(Message::Response(response))?;

        Ok(())
    }

    /// Handle requests from the server (like configuration requests)
    #[expect(dead_code)]
    fn handle_server_request(&mut self, request: lsp_server::Request) -> Result<()> {
        match request.method.as_str() {
            "workspace/configuration" => {
                let params: ConfigurationParams = serde_json::from_value(request.params)?;
                self.handle_workspace_configuration_request(request.id, &params)?;
            }
            "workspace/diagnostic/refresh" => {
                // TODO: Send diagnostic requests for all the open files to the server and send the
                // workspace diagnostics request if the server supports it.
                let response = Response::new_ok(request.id, serde_json::Value::Null);
                self.client_connection
                    .sender
                    .send(Message::Response(response))?;
            }
            "client/registerCapability" => {
                // TODO: We might have to expand this to handle more complex registrations and also
                // handle unregistration.
                let params: RegistrationParams = serde_json::from_value(request.params)?;
                for registration in params.registrations {
                    self.registered_capabilities.push(registration.method);
                }
                // Accept capability registration requests
                let response = Response::new_ok(request.id, serde_json::Value::Null);
                self.client_connection
                    .sender
                    .send(Message::Response(response))?;
            }
            _ => {
                return Err(TestServerError::UnrecognizedRequest(request).into());
            }
        }

        Ok(())
    }

    /// Get the initialization result
    pub(crate) fn initialization_result(&self) -> Option<&InitializeResult> {
        self.initialize_response.as_ref()
    }

    /// Send a `textDocument/didOpen` notification
    pub(crate) fn open_text_document(
        &mut self,
        path: impl AsRef<SystemPath>,
        content: &impl ToString,
    ) -> Result<()> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Url::from_file_path(path.as_ref()).expect("Path must be a valid URL"),
                language_id: "python".to_string(),
                version: self.next_document_version(),
                text: content.to_string(),
            },
        };
        self.send_notification::<DidOpenTextDocument>(params)
    }

    /// Send a `textDocument/didChange` notification with the given content changes
    #[expect(dead_code)]
    pub(crate) fn change_text_document(
        &mut self,
        path: impl AsRef<SystemPath>,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<()> {
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: Url::from_file_path(path.as_ref()).expect("Path must be a valid URL"),
                version: self.next_document_version(),
            },
            content_changes: changes,
        };
        self.send_notification::<DidChangeTextDocument>(params)
    }

    /// Send a `textDocument/didClose` notification
    #[expect(dead_code)]
    pub(crate) fn close_text_document(&mut self, path: impl AsRef<SystemPath>) -> Result<()> {
        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(path.as_ref()).expect("Path must be a valid URL"),
            },
        };
        self.send_notification::<DidCloseTextDocument>(params)
    }

    /// Send a `workspace/didChangeWatchedFiles` notification with the given file events
    #[expect(dead_code)]
    pub(crate) fn did_change_watched_files(&mut self, events: Vec<FileEvent>) -> Result<()> {
        let params = DidChangeWatchedFilesParams { changes: events };
        self.send_notification::<DidChangeWatchedFiles>(params)
    }

    /// Send a `textDocument/diagnostic` request for the document at the given path.
    pub(crate) fn document_diagnostic_request(
        &mut self,
        path: impl AsRef<SystemPath>,
    ) -> Result<DocumentDiagnosticReportResult> {
        let uri = Url::from_file_path(path.as_ref()).expect("Path must be a valid URL");
        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier { uri },
            identifier: Some("ty".to_string()),
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let id = self.send_request::<DocumentDiagnosticRequest>(params)?;
        self.get_response::<DocumentDiagnosticReportResult>(id)
    }
}

/// Builder for creating test servers with specific configurations
pub(crate) struct TestServerBuilder {
    workspace_folders: Vec<WorkspaceFolder>,
    workspace_configurations: HashMap<Url, ClientOptions>,
    memory_system: InMemorySystem,
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
            workspace_folders: Vec::new(),
            workspace_configurations: HashMap::new(),
            memory_system: InMemorySystem::default(),
            client_capabilities,
        }
    }

    /// Add a workspace configuration
    pub(crate) fn with_workspace(
        mut self,
        workspace_root: &SystemPath,
        options: ClientOptions,
    ) -> Self {
        let workspace_folder = WorkspaceFolder {
            uri: Url::from_file_path(workspace_root.as_std_path())
                .expect("workspace root must be a valid URL"),
            name: workspace_root.file_name().unwrap_or("test").to_string(),
        };
        self.workspace_configurations
            .insert(workspace_folder.uri.clone(), options);
        self.workspace_folders.push(workspace_folder);
        self
    }

    /// Set the in-memory system
    pub(crate) fn with_memory_system(mut self, memory_system: InMemorySystem) -> Self {
        self.memory_system = memory_system;
        self
    }

    /// Enable or disable pull diagnostics capability
    pub(crate) fn with_pull_diagnostics(mut self, enabled: bool) -> Self {
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
    pub(crate) fn with_did_change_watched_files(mut self, enabled: bool) -> Self {
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

    /// Build the test server
    pub(crate) fn build(self) -> Result<TestServer> {
        TestServer::new(
            self.workspace_folders,
            self.workspace_configurations,
            self.memory_system,
            self.client_capabilities,
        )
    }
}
