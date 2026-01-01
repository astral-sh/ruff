use crate::server::schedule::Scheduler;
use crate::server::{Server, api};
use crate::session::client::{Client, ClientResponseHandler};
use crate::session::{ClientOptions, SuspendedWorkspaceDiagnosticRequest};
use anyhow::anyhow;
use crossbeam::select;
use lsp_server::Message;
use lsp_types::notification::Notification;
use lsp_types::{ConfigurationParams, Url};
use serde_json::Value;

pub(crate) type ConnectionSender = crossbeam::channel::Sender<Message>;
pub(crate) type MainLoopSender = crossbeam::channel::Sender<Event>;
pub(crate) type MainLoopReceiver = crossbeam::channel::Receiver<Event>;

impl Server {
    pub(super) fn main_loop(&mut self) -> crate::Result<()> {
        self.initialize(&Client::new(
            self.main_loop_sender.clone(),
            self.connection.sender.clone(),
        ));

        let mut scheduler = Scheduler::new(self.worker_threads);

        while let Ok(next_event) = self.next_event() {
            let Some(next_event) = next_event else {
                anyhow::bail!("client exited without proper shutdown sequence");
            };

            let client = Client::new(
                self.main_loop_sender.clone(),
                self.connection.sender.clone(),
            );

            match next_event {
                Event::Message(msg) => {
                    let Some(msg) = self.session.should_defer_message(msg) else {
                        continue;
                    };

                    let task = match msg {
                        Message::Request(req) => {
                            self.session
                                .request_queue_mut()
                                .incoming_mut()
                                .register(req.id.clone(), req.method.clone());

                            if self.session.is_shutdown_requested() {
                                tracing::warn!(
                                    "Received request `{}` after server shutdown was requested, discarding",
                                    &req.method
                                );
                                client.respond_err(
                                    req.id,
                                    lsp_server::ResponseError {
                                        code: lsp_server::ErrorCode::InvalidRequest as i32,
                                        message: "Shutdown already requested".to_owned(),
                                        data: None,
                                    },
                                );
                                continue;
                            }

                            api::request(req)
                        }
                        Message::Notification(notification) => {
                            if notification.method == lsp_types::notification::Exit::METHOD {
                                if !self.session.is_shutdown_requested() {
                                    return Err(anyhow!(
                                        "Received exit notification before a shutdown request"
                                    ));
                                }

                                tracing::debug!("Received exit notification, exiting");
                                return Ok(());
                            }

                            api::notification(notification)
                        }

                        // Handle the response from the client to a server request
                        Message::Response(response) => {
                            if let Some(handler) = self
                                .session
                                .request_queue_mut()
                                .outgoing_mut()
                                .complete(&response.id)
                            {
                                handler.handle_response(&client, response);
                            } else {
                                tracing::error!(
                                    "Received a response with ID {}, which was not expected",
                                    response.id
                                );
                            }

                            continue;
                        }
                    };

                    scheduler.dispatch(task, &mut self.session, client);
                }
                Event::Action(action) => match action {
                    Action::SendResponse(response) => {
                        // Filter out responses for already canceled requests.
                        if let Some((start_time, method)) = self
                            .session
                            .request_queue_mut()
                            .incoming_mut()
                            .complete(&response.id)
                        {
                            let duration = start_time.elapsed();
                            tracing::debug!(name: "message response", method, %response.id, duration = format_args!("{:0.2?}", duration));

                            self.connection.sender.send(Message::Response(response))?;
                        } else {
                            tracing::debug!(
                                "Ignoring response for canceled request id={}",
                                response.id
                            );
                        }
                    }

                    Action::RetryRequest(request) => {
                        // Never retry canceled requests.
                        if self
                            .session
                            .request_queue()
                            .incoming()
                            .is_pending(&request.id)
                        {
                            let task = api::request(request);
                            scheduler.dispatch(task, &mut self.session, client);
                        } else {
                            tracing::debug!(
                                "Request {}/{} was cancelled, not retrying",
                                request.method,
                                request.id
                            );
                        }
                    }

                    Action::SendRequest(request) => client.send_request_raw(&self.session, request),

                    Action::SuspendWorkspaceDiagnostics(suspended_request) => {
                        self.session.set_suspended_workspace_diagnostics_request(
                            *suspended_request,
                            &client,
                        );
                    }

                    Action::InitializeWorkspaces(workspaces_with_options) => {
                        self.session
                            .initialize_workspaces(workspaces_with_options, &client);
                        // We do this here after workspaces have been initialized
                        // so that the file watcher globs can take project search
                        // paths into account.
                        // self.try_register_file_watcher(&client);
                    }
                },
            }
        }

        Ok(())
    }

    /// Waits for the next message from the client or action.
    ///
    /// Returns `Ok(None)` if the client connection is closed.
    fn next_event(&mut self) -> Result<Option<Event>, crossbeam::channel::RecvError> {
        // We can't queue those into the main loop because that could result in reordering if
        // the `select` below picks a client message first.
        if let Some(deferred) = self.session.take_deferred_messages() {
            match &deferred {
                Message::Request(req) => {
                    tracing::debug!("Processing deferred request `{}`", req.method);
                }
                Message::Notification(notification) => {
                    tracing::debug!("Processing deferred notification `{}`", notification.method);
                }
                Message::Response(response) => {
                    tracing::debug!("Processing deferred response `{}`", response.id);
                }
            }

            return Ok(Some(Event::Message(deferred)));
        }

        select!(
            recv(self.connection.receiver) -> msg => {
                // Ignore disconnect errors, they're handled by the main loop (it will exit).
                Ok(msg.ok().map(Event::Message))
            },
            recv(self.main_loop_receiver) -> event => event.map(Some),
        )
    }

    fn initialize(&mut self, client: &Client) {
        self.request_workspace_configurations(client);
    }

    /// Requests workspace configurations from the client for all the workspaces in the session.
    ///
    /// If the client does not support workspace configuration, it initializes the workspaces
    /// using the initialization options provided by the client.
    fn request_workspace_configurations(&mut self, client: &Client) {
        if !self
            .session
            .client_capabilities()
            .supports_workspace_configuration()
        {
            tracing::info!(
                "Client does not support workspace configuration, initializing workspaces \
                using the initialization options"
            );
            self.session.initialize_workspaces(
                self.session
                    .workspaces()
                    .urls()
                    .cloned()
                    .map(|url| (url, self.session.initialization_options().options.clone()))
                    .collect::<Vec<_>>(),
                client,
            );
            return;
        }

        let urls = self
            .session
            .workspaces()
            .urls()
            .cloned()
            .collect::<Vec<_>>();

        let items = urls
            .iter()
            .map(|root| lsp_types::ConfigurationItem {
                scope_uri: Some(root.clone()),
                section: Some("ty".to_string()),
            })
            .collect();

        tracing::debug!("Requesting workspace configuration for workspaces");
        client.send_request::<lsp_types::request::WorkspaceConfiguration>(
            &self.session,
            ConfigurationParams { items },
            |client, result: Vec<Value>| {
                tracing::debug!("Received workspace configurations, initializing workspaces");

                // This shouldn't fail because, as per the spec, the client needs to provide a
                // `null` value even if it cannot provide a configuration for a workspace.
                assert_eq!(
                    result.len(),
                    urls.len(),
                    "Mismatch in number of workspace URLs ({}) and configuration results ({})",
                    urls.len(),
                    result.len()
                );

                let workspaces_with_options: Vec<_> = urls
                    .into_iter()
                    .zip(result)
                    .map(|(url, value)| {
                        if value.is_null() {
                            tracing::debug!(
                                "No workspace options provided for {url}, using default options"
                            );
                            return (url, ClientOptions::default());
                        }
                        let options: ClientOptions =
                            serde_json::from_value(value).unwrap_or_else(|err| {
                                tracing::error!(
                                    "Failed to deserialize workspace options for {url}: {err}. \
                                        Using default options"
                                );
                                ClientOptions::default()
                            });
                        (url, options)
                    })
                    .collect();

                client.queue_action(Action::InitializeWorkspaces(workspaces_with_options));
            },
        );
    }
}

/// An action that should be performed on the main loop.
#[derive(Debug)]
pub(crate) enum Action {
    /// Send a response to the client
    SendResponse(lsp_server::Response),

    /// Retry a request that previously failed due to a salsa cancellation.
    RetryRequest(lsp_server::Request),

    /// Send a request from the server to the client.
    SendRequest(SendRequest),

    SuspendWorkspaceDiagnostics(Box<SuspendedWorkspaceDiagnosticRequest>),

    /// Initialize the workspace after the server received
    /// the options from the client.
    InitializeWorkspaces(Vec<(Url, ClientOptions)>),
}

#[derive(Debug)]
pub(crate) enum Event {
    /// An incoming message from the LSP client.
    Message(lsp_server::Message),

    Action(Action),
}

pub(crate) struct SendRequest {
    pub(crate) method: String,
    pub(crate) params: serde_json::Value,
    pub(crate) response_handler: ClientResponseHandler,
}

impl std::fmt::Debug for SendRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendRequest")
            .field("method", &self.method)
            .field("params", &self.params)
            .finish_non_exhaustive()
    }
}
