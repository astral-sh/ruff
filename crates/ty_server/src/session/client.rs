use crate::Session;
use crate::server::{Action, ConnectionSender, SendRequest};
use crate::server::{Event, MainLoopSender};
use lsp_server::{ErrorCode, Message, Notification, RequestId, ResponseError};
use serde_json::Value;
use std::any::TypeId;
use std::fmt::Display;

#[derive(Debug, Clone)]
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
    /// Use [`self.send_deferred_request`] if you are in a background task
    /// where you don't have access to the session. But note, that the
    /// request won't be send immediately, but rather queued up in the main loop
    pub(crate) fn send_request<R>(
        &self,
        session: &Session,
        params: R::Params,
        response_handler: impl FnOnce(&Client, R::Result) + Send + 'static,
    ) where
        R: lsp_types::request::Request,
    {
        self.send_request_raw(
            session,
            SendRequest {
                method: R::METHOD.to_string(),
                params: serde_json::to_value(params).expect("Params to be serializable"),
                response_handler: ClientResponseHandler::for_request::<R>(response_handler),
            },
        );
    }

    /// Sends a request of kind `R` to the client, with associated parameters.
    ///
    /// The request isn't sent immediately, but rather queued up in the main loop.
    /// The `response_handler` will be dispatched as soon as the client response
    /// is processed on the main-loop. The handler always runs on the main-loop thread.
    ///
    /// Use [`self.send_request`] if you are in a foreground task and have access to the session.
    pub(crate) fn send_deferred_request<R>(
        &self,
        params: R::Params,
        response_handler: impl FnOnce(&Client, R::Result) + Send + 'static,
    ) where
        R: lsp_types::request::Request,
    {
        self.main_loop_sender
            .send(Event::Action(Action::SendRequest(SendRequest {
                method: R::METHOD.to_string(),
                params: serde_json::to_value(params).expect("Params to be serializable"),
                response_handler: ClientResponseHandler::for_request::<R>(response_handler),
            })))
            .unwrap();
    }

    pub(crate) fn send_request_raw(&self, session: &Session, request: SendRequest) {
        let id = session
            .request_queue()
            .outgoing()
            .register(request.response_handler);

        if let Err(err) = self
            .client_sender
            .send(Message::Request(lsp_server::Request {
                id,
                method: request.method.clone(),
                params: request.params,
            }))
        {
            tracing::error!(
                "Failed to send request `{}` because the client sender is closed: {err}",
                request.method
            );
        }
    }

    /// Sends a notification to the client.
    pub(crate) fn send_notification<N>(&self, params: N::Params)
    where
        N: lsp_types::notification::Notification,
    {
        if let Err(err) =
            self.client_sender
                .send(lsp_server::Message::Notification(Notification::new(
                    N::METHOD.to_string(),
                    params,
                )))
        {
            tracing::error!(
                "Failed to send notification `{method}` because the client sender is closed: {err}",
                method = N::METHOD,
            );
        }
    }

    /// Sends a notification without any parameters to the client.
    ///
    /// This is useful for notifications that don't require any data.
    #[expect(dead_code)]
    pub(crate) fn send_notification_no_params(&self, method: &str) {
        if let Err(err) =
            self.client_sender
                .send(lsp_server::Message::Notification(Notification::new(
                    method.to_string(),
                    Value::Null,
                )))
        {
            tracing::error!(
                "Failed to send notification `{method}` because the client sender is closed: {err}",
            );
        }
    }

    /// Sends a response to the client for a given request ID.
    ///
    /// The response isn't sent immediately. Instead, it's queued up in the main loop
    /// and checked for cancellation (each request must have exactly one response).
    pub(crate) fn respond<R>(&self, id: &RequestId, result: crate::server::Result<R>)
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
            .unwrap();
    }

    /// Sends an error response to the client for a given request ID.
    ///
    /// The response isn't sent immediately. Instead, it's queued up in the main loop.
    pub(crate) fn respond_err(&self, id: RequestId, error: lsp_server::ResponseError) {
        let response = lsp_server::Response {
            id,
            result: None,
            error: Some(error),
        };

        self.main_loop_sender
            .send(Event::Action(Action::SendResponse(response)))
            .unwrap();
    }

    /// Shows a message to the user.
    ///
    /// This opens a pop up in VS Code showing `message`.
    pub(crate) fn show_message(&self, message: impl Display, message_type: lsp_types::MessageType) {
        self.send_notification::<lsp_types::notification::ShowMessage>(
            lsp_types::ShowMessageParams {
                typ: message_type,
                message: message.to_string(),
            },
        );
    }

    /// Sends a request to display a warning to the client with a formatted message. The warning is
    /// sent in a `window/showMessage` notification.
    ///
    /// Logs an error if the message could not be sent.
    pub(crate) fn show_warning_message(&self, message: impl Display) {
        self.show_message(message, lsp_types::MessageType::WARNING);
    }

    /// Sends a request to display an error to the client with a formatted message. The error is
    /// sent in a `window/showMessage` notification.
    ///
    /// Logs an error if the message could not be sent.
    pub(crate) fn show_error_message(&self, message: impl Display) {
        self.show_message(message, lsp_types::MessageType::ERROR);
    }

    /// Re-queues this request after a salsa cancellation for a retry.
    ///
    /// The main loop will skip the retry if the client cancelled the request in the  meantime.
    pub(crate) fn retry(&self, request: lsp_server::Request) {
        self.main_loop_sender
            .send(Event::Action(Action::RetryRequest(request)))
            .unwrap();
    }

    pub(crate) fn queue_action(&self, action: Action) {
        self.main_loop_sender.send(Event::Action(action)).unwrap();
    }

    pub(crate) fn cancel(&self, session: &mut Session, id: RequestId) {
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
            if let Err(err) = self
                .client_sender
                .send(Message::Response(lsp_server::Response {
                    id,
                    result: None,
                    error: Some(error),
                }))
            {
                tracing::error!(
                    "Failed to send cancellation response for request `{method_name}` because the client sender is closed: {err}",
                );
            }
        }
    }
}

/// Type erased handler for client responses.
#[allow(clippy::type_complexity)]
pub(crate) struct ClientResponseHandler(Box<dyn FnOnce(&Client, lsp_server::Response) + Send>);

impl ClientResponseHandler {
    fn for_request<R>(response_handler: impl FnOnce(&Client, R::Result) + Send + 'static) -> Self
    where
        R: lsp_types::request::Request,
    {
        Self(Box::new(
            move |client: &Client, response: lsp_server::Response| {
                let _span =
                    tracing::debug_span!("client_response", id=%response.id, method = R::METHOD)
                        .entered();

                match (response.error, response.result) {
                    (Some(err), _) => {
                        tracing::error!(
                            "Got an error from the client (code {code}, method {method}): {message}",
                            code = err.code,
                            message = &err.message,
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
            },
        ))
    }

    pub(crate) fn handle_response(self, client: &Client, response: lsp_server::Response) {
        let handler = self.0;
        handler(client, response);
    }
}
