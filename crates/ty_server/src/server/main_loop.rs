use crate::server::schedule::Scheduler;
use crate::server::{Server, api};
use crate::session::client::Client;
use anyhow::anyhow;
use crossbeam::select;
use lsp_server::Message;
use lsp_types::notification::Notification;
use lsp_types::{DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher};

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

            match next_event {
                Event::Message(msg) => {
                    let client = Client::new(
                        self.main_loop_sender.clone(),
                        self.connection.sender.clone(),
                    );

                    let task = match msg {
                        Message::Request(req) => {
                            self.session
                                .request_queue_mut()
                                .incoming_mut()
                                .register(req.id.clone(), req.method.clone());

                            if self.session.is_shutdown_requested() {
                                tracing::warn!(
                                    "Received request after server shutdown was requested, discarding"
                                );
                                client.respond_err(
                                    req.id,
                                    lsp_server::ResponseError {
                                        code: lsp_server::ErrorCode::InvalidRequest as i32,
                                        message: "Shutdown already requested".to_owned(),
                                        data: None,
                                    },
                                )?;
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
                                handler(&client, response);
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
                            tracing::trace!(name: "message response", method, %response.id, duration = format_args!("{:0.2?}", duration));

                            self.connection.sender.send(Message::Response(response))?;
                        } else {
                            tracing::trace!(
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
                            api::request(request);
                        } else {
                            tracing::debug!(
                                "Request {}/{} was cancelled, not retrying",
                                request.method,
                                request.id
                            );
                        }
                    }
                },
            }
        }

        Ok(())
    }

    /// Waits for the next message from the client or action.
    ///
    /// Returns `Ok(None)` if the client connection is closed.
    fn next_event(&self) -> Result<Option<Event>, crossbeam::channel::RecvError> {
        select!(
            recv(self.connection.receiver) -> msg => {
                // Ignore disconnect errors, they're handled by the main loop (it will exit).
                Ok(msg.ok().map(Event::Message))
            },
            recv(self.main_loop_receiver) -> event => event.map(Some),
        )
    }

    fn initialize(&mut self, client: &Client) {
        let fs_watcher = self
            .client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files?.dynamic_registration)
            .unwrap_or_default();

        if fs_watcher {
            let registration = lsp_types::Registration {
                id: "workspace/didChangeWatchedFiles".to_owned(),
                method: "workspace/didChangeWatchedFiles".to_owned(),
                register_options: Some(
                    serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                        watchers: vec![
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/ty.toml".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String(
                                    "**/.gitignore".into(),
                                ),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/.ignore".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String(
                                    "**/pyproject.toml".into(),
                                ),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.py".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.pyi".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.ipynb".into()),
                                kind: None,
                            },
                        ],
                    })
                    .unwrap(),
                ),
            };
            let response_handler = move |_: &Client, ()| {
                tracing::info!("File watcher successfully registered");
            };

            if let Err(err) = client.send_request::<lsp_types::request::RegisterCapability>(
                &self.session,
                lsp_types::RegistrationParams {
                    registrations: vec![registration],
                },
                response_handler,
            ) {
                tracing::error!(
                    "An error occurred when trying to register the configuration file watcher: {err}"
                );
            }
        } else {
            tracing::warn!("The client does not support file system watching.");
        }
    }
}

/// An action that should be performed on the main loop.
#[derive(Debug)]
pub(crate) enum Action {
    /// Send a response to the client
    SendResponse(lsp_server::Response),

    /// Retry a request that previously failed due to a salsa cancellation.
    RetryRequest(lsp_server::Request),
}

#[derive(Debug)]
pub(crate) enum Event {
    /// An incoming message from the LSP client.
    Message(lsp_server::Message),

    Action(Action),
}
