use crate::server::schedule::{Scheduler, Task};
use crate::server::{MAX_QUEUED_REQUESTS, Server, api};
use crate::session::client::{Client, ClientResponseHandler};
use crate::session::{ClientOptions, Session, SuspendedWorkspaceDiagnosticRequest};
use anyhow::anyhow;
use crossbeam::channel::TrySendError;
use lsp_server::{ErrorCode, Message, Request, ResponseError};
use lsp_types::Notification;
use lsp_types::Uri;
use ruff_db::system::SystemPathBuf;

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

                    match msg {
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
                                    ResponseError {
                                        code: ErrorCode::InvalidRequest as i32,
                                        message: "Shutdown already requested".to_owned(),
                                        data: None,
                                    },
                                );
                                continue;
                            }

                            dispatch_request(
                                &mut scheduler,
                                req,
                                &mut self.session,
                                &client,
                                RequestAttempt::Initial,
                            )?;
                        }
                        Message::Notification(notification) => {
                            if notification.method == lsp_types::ExitNotification::METHOD.as_str() {
                                if !self.session.is_shutdown_requested() {
                                    return Err(anyhow!(
                                        "Received exit notification before a shutdown request"
                                    ));
                                }

                                tracing::debug!("Received exit notification, exiting");
                                return Ok(());
                            }

                            scheduler.dispatch(
                                api::notification(notification),
                                &mut self.session,
                                &client,
                            )?;
                        }

                        // Handle the response from the client to a server request
                        Message::Response(response) => {
                            if let Some(handler) = self
                                .session
                                .request_queue_mut()
                                .outgoing_mut()
                                .complete(&response.id)
                            {
                                handler.handle_response(&client, &mut self.session, response);
                            } else {
                                tracing::error!(
                                    "Received a response with ID {}, which was not expected",
                                    response.id
                                );
                            }
                            continue;
                        }
                    }
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
                            dispatch_request(
                                &mut scheduler,
                                request,
                                &mut self.session,
                                &client,
                                RequestAttempt::Retry,
                            )?;
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
                            .initialize_workspace_folders(&client, workspaces_with_options);
                        // We do this here after workspaces have been initialized
                        // so that the file watcher globs can take project search
                        // paths into account.
                        // self.try_register_file_watcher(&client);
                    }
                },
                Event::PollUvEnvironments { project_root } => {
                    self.session.poll_uv_sync(&client, &project_root);
                }
            }
        }

        Ok(())
    }

    /// Waits for the next message from the client or action.
    ///
    /// Returns `Ok(None)` if the client connection is closed.
    fn next_event(&mut self) -> Result<Option<Event>, crossbeam::channel::RecvError> {
        // Deliver ready worker responses before replaying more deferred requests.
        if let Ok(event) = self.main_loop_receiver.try_recv() {
            return Ok(Some(event));
        }

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

        let uv_sync = UvSyncWakeups(self.session.uv_sync_wakeups());
        uv_sync.select(&self.connection.receiver, &self.main_loop_receiver)
    }

    fn initialize(&mut self, client: &Client) {
        self.session
            .request_uninitialized_workspace_folder_configurations(client);
    }
}

/// Runs a synchronous request on the main loop or tries to queue a background request.
///
/// If a hover arrives while the worker queue is full, we tell the editor that ty is busy
/// (`RequestFailed`). Waiting for a slot would prevent the main loop from reading the next
/// message, even if it's `shutdown`.
///
/// An edit can also interrupt a completion that's already running. Normally we retry it.
/// If the queue is full by then, we return the completion handler's `ContentModified` error
/// to the editor. Posting another retry could keep the main loop busy with that same
/// completion until a worker frees a slot.
fn dispatch_request(
    scheduler: &mut Scheduler,
    request: Request,
    session: &mut Session,
    client: &Client,
    attempt: RequestAttempt,
) -> crate::Result<()> {
    let id = request.id.clone();
    let task = api::request(request);
    let retry_error = match (&task, attempt) {
        (Task::Background(background), RequestAttempt::Retry) => background.cancellation_error,
        _ => None,
    };
    match scheduler.dispatch(task, session, client) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(())) => {
            tracing::warn!(
                %id,
                method = session.request_queue().incoming().method(&id),
                ?attempt,
                capacity = MAX_QUEUED_REQUESTS,
                "Worker queue is full, cannot schedule request"
            );
            let error = match retry_error {
                Some(cancelled) => cancelled(),
                None => ResponseError {
                    code: ErrorCode::RequestFailed as i32,
                    message: "ty is busy: too many queued requests".to_owned(),
                    data: None,
                },
            };
            client.respond_err(id, error);
            Ok(())
        }
        Err(TrySendError::Disconnected(())) => anyhow::bail!("Worker pool disconnected"),
    }
}

#[derive(Clone, Copy, Debug)]
enum RequestAttempt {
    Initial,
    Retry,
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
    InitializeWorkspaces(Vec<(Uri, ClientOptions)>),
}

#[derive(Debug)]
pub(crate) enum Event {
    /// An incoming message from the LSP client.
    Message(lsp_server::Message),

    Action(Action),

    PollUvEnvironments {
        project_root: SystemPathBuf,
    },
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

/// Uv-environment wakeups for the currently active project databases.
struct UvSyncWakeups(Vec<(SystemPathBuf, crossbeam::channel::Receiver<()>)>);

impl UvSyncWakeups {
    /// Waits for a main-loop action, project wakeup, or client message, in that order.
    fn select(
        &self,
        connection: &crossbeam::channel::Receiver<Message>,
        main_loop: &MainLoopReceiver,
    ) -> Result<Option<Event>, crossbeam::channel::RecvError> {
        let mut select = crossbeam::channel::Select::new_biased();
        let main_loop_index = select.recv(main_loop);
        for (_, receiver) in &self.0 {
            select.recv(receiver);
        }
        let connection_index = select.recv(connection);
        let operation = select.select();
        let index = operation.index();

        if index == main_loop_index {
            return operation.recv(main_loop).map(Some);
        }

        if let Some((project_root, receiver)) = index.checked_sub(1).and_then(|i| self.0.get(i)) {
            return operation.recv(receiver).map(|()| {
                Some(Event::PollUvEnvironments {
                    project_root: project_root.clone(),
                })
            });
        }

        debug_assert_eq!(index, connection_index);
        // Ignore disconnect errors, they're handled by the main loop (it will exit).
        Ok(operation.recv(connection).ok().map(Event::Message))
    }
}
