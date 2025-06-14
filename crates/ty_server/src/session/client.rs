use crate::Session;
use crate::server::{Action, ConnectionSender};
use crate::server::{Event, MainLoopSender};
use anyhow::{Context, anyhow};
use lsp_server::{ErrorCode, Message, Notification, RequestId, ResponseError};
use serde_json::Value;
use std::any::TypeId;
use std::fmt::Display;

pub(crate) type ClientResponseHandler = Box<dyn FnOnce(&Client, lsp_server::Response) + Send>;

#[derive(Debug)]
pub(crate) struct Client {
    /// Channel to send messages back to the main loop.
    main_loop_sender: MainLoopSender,
    /// Channel to send messages directly to the LSP client without going through the main loop.
    ///
    /// This is generally preferred because it reduces pressure on the main loop but it may not always be
    /// possible if access to data on [`Session`] is required, which background tasks don't have.
    client_sender: ConnectionSender,
}

impl Client {
    pub(crate) fn new(main_loop_sender: MainLoopSender, client_sender: ConnectionSender) -> Self {
        Self {
            main_loop_sender,
            client_sender,
        }
    }

    /// Sends a request of kind `R` to the client, with associated parameters.
    ///
    /// The request is sent immediately.
    /// The `response_handler` will be dispatched as soon as the client response
    /// is processed on the main-loop. The handler always runs on the main-loop thread.
    ///
    /// # Note
    /// This method takes a `session` so that we can register the pending-request
    /// and send the response directly to the client. If this ever becomes too limiting (because we
    /// need to send a request from somewhere where we don't have access to session), consider introducing
    /// a new `send_deferred_request` method that doesn't take a session and instead sends
    /// an `Action` to the main loop to send the request (the main loop has always access to session).
    pub(crate) fn send_request<R>(
        &self,
        session: &Session,
        params: R::Params,
        response_handler: impl FnOnce(&Client, R::Result) + Send + 'static,
    ) -> crate::Result<()>
    where
        R: lsp_types::request::Request,
    {
        let response_handler = Box::new(move |client: &Client, response: lsp_server::Response| {
            let _span =
                tracing::debug_span!("client_response", id=%response.id, method = R::METHOD)
                    .entered();

            match (response.error, response.result) {
                (Some(err), _) => {
                    tracing::error!(
                        "Got an error from the client (code {code}, method {method}): {message}",
                        code = err.code,
                        message = err.message,
                        method = R::METHOD
                    );
                }
                (None, Some(response)) => match serde_json::from_value(response) {
                    Ok(response) => response_handler(client, response),
                    Err(error) => {
                        tracing::error!(
                            "Failed to deserialize client response (method={method}): {error}",
                            method = R::METHOD
                        );
                    }
                },
                (None, None) => {
                    if TypeId::of::<R::Result>() == TypeId::of::<()>() {
                        // We can't call `response_handler(())` directly here, but
                        // since we _know_ the type expected is `()`, we can use
                        // `from_value(Value::Null)`. `R::Result` implements `DeserializeOwned`,
                        // so this branch works in the general case but we'll only
                        // hit it if the concrete type is `()`, so the `unwrap()` is safe here.
                        response_handler(client, serde_json::from_value(Value::Null).unwrap());
                    } else {
                        tracing::error!(
                            "Invalid client response: did not contain a result or error (method={method})",
                            method = R::METHOD
                        );
                    }
                }
            }
        });

        let id = session
            .request_queue()
            .outgoing()
            .register(response_handler);

        self.client_sender
            .send(Message::Request(lsp_server::Request {
                id,
                method: R::METHOD.to_string(),
                params: serde_json::to_value(params).context("Failed to serialize params")?,
            }))
            .with_context(|| {
                format!("Failed to send request method={method}", method = R::METHOD)
            })?;

        Ok(())
    }

    /// Sends a notification to the client.
    pub(crate) fn send_notification<N>(&self, params: N::Params) -> crate::Result<()>
    where
        N: lsp_types::notification::Notification,
    {
        let method = N::METHOD.to_string();

        self.client_sender
            .send(lsp_server::Message::Notification(Notification::new(
                method, params,
            )))
            .map_err(|error| {
                anyhow!(
                    "Failed to send notification (method={method}): {error}",
                    method = N::METHOD
                )
            })
    }

    /// Sends a notification without any parameters to the client.
    ///
    /// This is useful for notifications that don't require any data.
    #[expect(dead_code)]
    pub(crate) fn send_notification_no_params(&self, method: &str) -> crate::Result<()> {
        self.client_sender
            .send(lsp_server::Message::Notification(Notification::new(
                method.to_string(),
                Value::Null,
            )))
            .map_err(|error| anyhow!("Failed to send notification (method={method}): {error}",))
    }

    /// Sends a response to the client for a given request ID.
    ///
    /// The response isn't sent immediately. Instead, it's queued up in the main loop
    /// and checked for cancellation (each request must have exactly one response).
    pub(crate) fn respond<R>(
        &self,
        id: &RequestId,
        result: crate::server::Result<R>,
    ) -> crate::Result<()>
    where
        R: serde::Serialize,
    {
        let response = match result {
            Ok(res) => lsp_server::Response::new_ok(id.clone(), res),
            Err(crate::server::Error { code, error }) => {
                lsp_server::Response::new_err(id.clone(), code as i32, error.to_string())
            }
        };

        self.main_loop_sender
            .send(Event::Action(Action::SendResponse(response)))
            .map_err(|error| anyhow!("Failed to send response for request {id}: {error}"))
    }

    /// Sends an error response to the client for a given request ID.
    ///
    /// The response isn't sent immediately. Instead, it's queued up in the main loop.
    pub(crate) fn respond_err(
        &self,
        id: RequestId,
        error: lsp_server::ResponseError,
    ) -> crate::Result<()> {
        let response = lsp_server::Response {
            id,
            result: None,
            error: Some(error),
        };

        self.main_loop_sender
            .send(Event::Action(Action::SendResponse(response)))
            .map_err(|error| anyhow!("Failed to send response: {error}"))
    }

    /// Shows a message to the user.
    ///
    /// This opens a pop up in VS Code showing `message`.
    pub(crate) fn show_message(
        &self,
        message: impl Display,
        message_type: lsp_types::MessageType,
    ) -> crate::Result<()> {
        self.send_notification::<lsp_types::notification::ShowMessage>(
            lsp_types::ShowMessageParams {
                typ: message_type,
                message: message.to_string(),
            },
        )
    }

    /// Sends a request to display a warning to the client with a formatted message. The warning is
    /// sent in a `window/showMessage` notification.
    ///
    /// Logs an error if the message could not be sent.
    pub(crate) fn show_warning_message(&self, message: impl Display) {
        let result = self.show_message(message, lsp_types::MessageType::WARNING);

        if let Err(err) = result {
            tracing::error!("Failed to send warning message to the client: {err}");
        }
    }

    /// Sends a request to display an error to the client with a formatted message. The error is
    /// sent in a `window/showMessage` notification.
    ///
    /// Logs an error if the message could not be sent.
    pub(crate) fn show_error_message(&self, message: impl Display) {
        let result = self.show_message(message, lsp_types::MessageType::ERROR);

        if let Err(err) = result {
            tracing::error!("Failed to send error message to the client: {err}");
        }
    }

    /// Re-queues this request after a salsa cancellation for a retry.
    ///
    /// The main loop will skip the retry if the client cancelled the request in the  meantime.
    pub(crate) fn retry(&self, request: lsp_server::Request) -> crate::Result<()> {
        self.main_loop_sender
            .send(Event::Action(Action::RetryRequest(request)))
            .map_err(|error| anyhow!("Failed to send retry request: {error}"))
    }

    pub(crate) fn cancel(&self, session: &mut Session, id: RequestId) -> crate::Result<()> {
        let method_name = session.request_queue_mut().incoming_mut().cancel(&id);

        if let Some(method_name) = method_name {
            tracing::debug!("Cancelled request id={id} method={method_name}");
            let error = ResponseError {
                code: ErrorCode::RequestCanceled as i32,
                message: "request was cancelled by client".to_owned(),
                data: None,
            };

            // Use `client_sender` here instead of `respond_err` because
            // `respond_err` filters out responses for canceled requests (which we just did!).
            self.client_sender
                .send(Message::Response(lsp_server::Response {
                    id,
                    result: None,
                    error: Some(error),
                }))?;
        }

        Ok(())
    }
}
