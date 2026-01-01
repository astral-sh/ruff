use crate::server::schedule::Task;
use crate::session::Session;
use anyhow::anyhow;
use lsp_server as server;
use lsp_server::{ErrorCode, RequestId};
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use std::panic::{AssertUnwindSafe, UnwindSafe};

mod diagnostics;
mod notifications;
mod requests;
mod semantic_tokens;
mod symbols;
mod traits;

use self::traits::{NotificationHandler, RequestHandler};
use super::{Result, schedule::BackgroundSchedule};
use crate::session::client::Client;
pub(crate) use diagnostics::publish_settings_diagnostics;
pub use requests::{PartialWorkspaceProgress, PartialWorkspaceProgressParams};
use ruff_db::panic::PanicError;

/// Processes a request from the client to the server.
///
/// The LSP specification requires that each request has exactly one response. Therefore,
/// it's crucial that all paths in this method call [`Client::respond`] exactly once.
/// The only exception to this is requests that were cancelled by the client. In this case,
/// the response was already sent by the [`notification::CancelNotificationHandler`].
pub(super) fn request(req: server::Request) -> Task {
    let id = req.id.clone();

    match req.method.as_str() {
        requests::ExecuteCommand::METHOD => sync_request_task::<requests::ExecuteCommand>(req),
        requests::CodeActionRequestHandler::METHOD => background_document_request_task::<
            requests::CodeActionRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::DocumentDiagnosticRequestHandler::METHOD => background_document_request_task::<
            requests::DocumentDiagnosticRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::WorkspaceDiagnosticRequestHandler::METHOD => background_request_task::<
            requests::WorkspaceDiagnosticRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::GotoTypeDefinitionRequestHandler::METHOD => background_document_request_task::<
            requests::GotoTypeDefinitionRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::GotoDeclarationRequestHandler::METHOD => background_document_request_task::<
            requests::GotoDeclarationRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::GotoDefinitionRequestHandler::METHOD => background_document_request_task::<
            requests::GotoDefinitionRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::HoverRequestHandler::METHOD => background_document_request_task::<
            requests::HoverRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::ReferencesRequestHandler::METHOD => background_document_request_task::<
            requests::ReferencesRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::DocumentHighlightRequestHandler::METHOD => background_document_request_task::<
            requests::DocumentHighlightRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::InlayHintRequestHandler::METHOD => background_document_request_task::<
            requests::InlayHintRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::SemanticTokensRequestHandler::METHOD => background_document_request_task::<
            requests::SemanticTokensRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::SemanticTokensRangeRequestHandler::METHOD => background_document_request_task::<
            requests::SemanticTokensRangeRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::SignatureHelpRequestHandler::METHOD => background_document_request_task::<
            requests::SignatureHelpRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::PrepareRenameRequestHandler::METHOD => background_document_request_task::<
            requests::PrepareRenameRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::RenameRequestHandler::METHOD => background_document_request_task::<
            requests::RenameRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::CompletionRequestHandler::METHOD => background_document_request_task::<
            requests::CompletionRequestHandler,
        >(
            req, BackgroundSchedule::LatencySensitive
        ),
        requests::SelectionRangeRequestHandler::METHOD => background_document_request_task::<
            requests::SelectionRangeRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::DocumentSymbolRequestHandler::METHOD => background_document_request_task::<
            requests::DocumentSymbolRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::WorkspaceSymbolRequestHandler::METHOD => background_request_task::<
            requests::WorkspaceSymbolRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        lsp_types::request::Shutdown::METHOD => sync_request_task::<requests::ShutdownHandler>(req),

        method => {
            tracing::warn!("Received request {method} which does not have a handler");
            let result: Result<()> = Err(Error::new(
                anyhow!("Unknown request: {method}"),
                server::ErrorCode::MethodNotFound,
            ));
            return Task::immediate(id, result);
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing request with ID {id}: {err}");

        Task::sync(move |_session, client| {
            if matches!(err.code, ErrorCode::InternalError) {
                client.show_error_message("ty failed to handle a request from the editor. Check the logs for more details.");
            }

            respond_silent_error(
                id,
                client,
                lsp_server::ResponseError {
                    code: err.code as i32,
                    message: err.to_string(),
                    data: None,
                },
            );
        })
    })
}

pub(super) fn notification(notif: server::Notification) -> Task {
    match notif.method.as_str() {
        notifications::DidCloseTextDocumentHandler::METHOD => {
            sync_notification_task::<notifications::DidCloseTextDocumentHandler>(notif)
        }
        notifications::DidOpenTextDocumentHandler::METHOD => {
            sync_notification_task::<notifications::DidOpenTextDocumentHandler>(notif)
        }
        notifications::DidChangeTextDocumentHandler::METHOD => {
            sync_notification_task::<notifications::DidChangeTextDocumentHandler>(notif)
        }
        notifications::DidOpenNotebookHandler::METHOD => {
            sync_notification_task::<notifications::DidOpenNotebookHandler>(notif)
        }
        notifications::DidChangeNotebookHandler::METHOD => {
            sync_notification_task::<notifications::DidChangeNotebookHandler>(notif)
        }
        notifications::DidCloseNotebookHandler::METHOD => {
            sync_notification_task::<notifications::DidCloseNotebookHandler>(notif)
        }
        notifications::DidChangeWatchedFiles::METHOD => {
            sync_notification_task::<notifications::DidChangeWatchedFiles>(notif)
        }
        lsp_types::notification::Cancel::METHOD => {
            sync_notification_task::<notifications::CancelNotificationHandler>(notif)
        }
        lsp_types::notification::SetTrace::METHOD => {
            tracing::trace!("Ignoring `setTrace` notification");
            return Task::nothing();
        }

        method => {
            tracing::warn!("Received notification {method} which does not have a handler.");
            return Task::nothing();
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing notification: {err}");
        Task::sync(move |_session, client| {
            if matches!(err.code, ErrorCode::InternalError) {
                client.show_error_message(
                    "ty failed to handle a notification from the editor. Check the logs for more details."
                );
            }
        })
    })
}

fn sync_request_task<R: traits::SyncRequestHandler>(req: server::Request) -> Result<Task>
where
    <<R as RequestHandler>::RequestType as Request>::Params: UnwindSafe,
{
    let (id, params) = cast_request::<R>(req)?;
    Ok(Task::sync(move |session, client: &Client| {
        let _span = tracing::debug_span!("request", %id, method = R::METHOD).entered();
        let result = R::run(session, client, params);
        respond::<R>(&id, result, client);
    }))
}

fn background_request_task<R: traits::BackgroundRequestHandler>(
    req: server::Request,
    schedule: BackgroundSchedule,
) -> Result<Task>
where
    <<R as RequestHandler>::RequestType as Request>::Params: UnwindSafe,
{
    let retry = R::RETRY_ON_CANCELLATION.then(|| req.clone());
    let (id, params) = cast_request::<R>(req)?;

    Ok(Task::background(schedule, move |session: &Session| {
        let cancellation_token = session
            .request_queue()
            .incoming()
            .cancellation_token(&id)
            .expect("request should have been tested for cancellation before scheduling");

        // SAFETY: The `snapshot` is safe to move across the unwind boundary because it is not used
        // after unwinding.
        let snapshot = AssertUnwindSafe(session.snapshot_session());

        Box::new(move |client| {
            let _span = tracing::debug_span!("request", %id, method = R::METHOD).entered();

            // Test again if the request was cancelled since it was scheduled on the background task
            // and, if so, return early
            if cancellation_token.is_cancelled() {
                tracing::debug!(
                    "Ignoring request id={id} method={} because it was cancelled",
                    R::METHOD
                );

                // We don't need to send a response here because the `cancel` notification
                // handler already responded with a message.
                return;
            }

            if let Err(error) = ruff_db::panic::catch_unwind(|| {
                let snapshot = snapshot;
                R::handle_request(&id, snapshot.0, client, params);
            }) {
                panic_response::<R>(&id, client, &error, retry);
            }
        })
    }))
}

fn background_document_request_task<R: traits::BackgroundDocumentRequestHandler>(
    req: server::Request,
    schedule: BackgroundSchedule,
) -> Result<Task>
where
    <<R as RequestHandler>::RequestType as Request>::Params: UnwindSafe,
{
    let retry = R::RETRY_ON_CANCELLATION.then(|| req.clone());
    let (id, params) = cast_request::<R>(req)?;

    Ok(Task::background(schedule, move |session: &Session| {
        let cancellation_token = session
            .request_queue()
            .incoming()
            .cancellation_token(&id)
            .expect("request should have been tested for cancellation before scheduling");

        let url = R::document_url(&params);

        let Ok(document) = session.snapshot_document(&url) else {
            let reason = format!("Document {url} is not open in the session");
            tracing::warn!(
                "Ignoring request id={id} method={} because {reason}",
                R::METHOD
            );
            return Box::new(|client| {
                respond_silent_error(
                    id,
                    client,
                    lsp_server::ResponseError {
                        code: lsp_server::ErrorCode::InvalidParams as i32,
                        message: reason,
                        data: None,
                    },
                );
            });
        };

        let path = document.notebook_or_file_path();
        let db = session.project_db(path).clone();

        Box::new(move |client| {
            let _span = tracing::debug_span!("request", %id, method = R::METHOD).entered();

            // Test again if the request was cancelled since it was scheduled on the background task
            // and, if so, return early
            if cancellation_token.is_cancelled() {
                tracing::debug!(
                    "Ignoring request id={id} method={} because it was cancelled",
                    R::METHOD
                );

                // We don't need to send a response here because the `cancel` notification
                // handler already responded with a message.
                return;
            }

            if let Err(error) = ruff_db::panic::catch_unwind(|| {
                salsa::attach(&db, || {
                    R::handle_request(&id, &db, document, client, params);
                });
            }) {
                panic_response::<R>(&id, client, &error, retry);
            }
        })
    }))
}

fn panic_response<R>(
    id: &RequestId,
    client: &Client,
    error: &PanicError,
    request: Option<lsp_server::Request>,
) where
    R: traits::RetriableRequestHandler,
{
    // Check if the request was canceled due to some modifications to the salsa database.
    if error.payload.downcast_ref::<salsa::Cancelled>().is_some() {
        // If the query supports retry, re-queue the request.
        // The query is still likely to succeed if the user modified any other document.
        if let Some(request) = request {
            tracing::debug!(
                "request id={} method={} was cancelled by salsa, re-queueing for retry",
                request.id,
                request.method
            );
            client.retry(request);
        } else {
            tracing::debug!(
                "request id={} was cancelled by salsa, sending content modified",
                id
            );
            respond_silent_error(id.clone(), client, R::salsa_cancellation_error());
        }
    } else {
        respond::<R>(
            id,
            Err(Error {
                code: lsp_server::ErrorCode::InternalError,
                error: anyhow!("request handler {error}"),
            }),
            client,
        );
    }
}

fn sync_notification_task<N: traits::SyncNotificationHandler>(
    notif: server::Notification,
) -> Result<Task> {
    let (id, params) = cast_notification::<N>(notif)?;
    Ok(Task::sync(move |session, client| {
        let _span = tracing::debug_span!("notification", method = N::METHOD).entered();
        if let Err(err) = N::run(session, client, params) {
            tracing::error!("An error occurred while running {id}: {err}");
            client.show_error_message("ty encountered a problem. Check the logs for more details.");

            return;
        }

        // If there's a pending workspace diagnostic long-polling request,
        // resume it, but only if the session revision changed (e.g. because some document changed).
        session.resume_suspended_workspace_diagnostic_request(client);
    }))
}

#[expect(dead_code)]
fn background_notification_thread<N>(
    req: server::Notification,
    schedule: BackgroundSchedule,
) -> Result<Task>
where
    N: traits::BackgroundDocumentNotificationHandler,
    <<N as NotificationHandler>::NotificationType as Notification>::Params: UnwindSafe,
{
    let (id, params) = cast_notification::<N>(req)?;
    Ok(Task::background(schedule, move |session: &Session| {
        let url = N::document_url(&params);
        let Ok(snapshot) = session.snapshot_document(&url) else {
            let reason = format!("Document {url} is not open in the session");
            tracing::warn!(
                "Ignoring notification id={id} method={} because {reason}",
                N::METHOD
            );
            return Box::new(|_| {});
        };

        Box::new(move |client| {
            let _span = tracing::debug_span!("notification", method = N::METHOD).entered();

            let result = match ruff_db::panic::catch_unwind(|| {
                N::run_with_snapshot(snapshot, client, params)
            }) {
                Ok(result) => result,
                Err(panic) => {
                    tracing::error!("An error occurred while running {id}: {panic}");
                    client.show_error_message(
                        "ty encountered a panic. Check the logs for more details.",
                    );
                    return;
                }
            };

            if let Err(err) = result {
                tracing::error!("An error occurred while running {id}: {err}");
                client.show_error_message(
                    "ty encountered a problem. Check the logs for more details.",
                );
            }
        })
    }))
}

/// Tries to cast a serialized request from the server into
/// a parameter type for a specific request handler.
/// It is *highly* recommended to not override this function in your
/// implementation.
fn cast_request<Req>(
    request: server::Request,
) -> Result<(
    RequestId,
    <<Req as RequestHandler>::RequestType as Request>::Params,
)>
where
    Req: RequestHandler,
    <<Req as RequestHandler>::RequestType as Request>::Params: UnwindSafe,
{
    request
        .extract(Req::METHOD)
        .map_err(|err| match err {
            json_err @ server::ExtractError::JsonError { .. } => {
                anyhow::anyhow!("JSON parsing failure:\n{json_err}")
            }
            server::ExtractError::MethodMismatch(_) => {
                unreachable!("A method mismatch should not be possible here unless you've used a different handler (`Req`) \
                    than the one whose method name was matched against earlier.")
            }
        })
        .with_failure_code(server::ErrorCode::InvalidParams)
}

/// Sends back a response to the client, but only if the request wasn't cancelled.
fn respond<Req>(
    id: &RequestId,
    result: Result<<<Req as RequestHandler>::RequestType as Request>::Result>,
    client: &Client,
) where
    Req: RequestHandler,
{
    if let Err(err) = &result {
        tracing::error!("An error occurred with request ID {id}: {err}");
        client.show_error_message("ty encountered a problem. Check the logs for more details.");
    }
    client.respond(id, result);
}

/// Sends back an error response to the server using a [`Client`] without showing a warning
/// to the user.
fn respond_silent_error(id: RequestId, client: &Client, error: lsp_server::ResponseError) {
    client.respond_err(id, error);
}

/// Tries to cast a serialized request from the server into
/// a parameter type for a specific request handler.
fn cast_notification<N>(
    notification: server::Notification,
) -> Result<(
    &'static str,
    <<N as NotificationHandler>::NotificationType as Notification>::Params,
)>
where
    N: NotificationHandler,
{
    Ok((
        N::METHOD,
        notification
            .extract(N::METHOD)
            .map_err(|err| match err {
                json_err @ server::ExtractError::JsonError { .. } => {
                    anyhow::anyhow!("JSON parsing failure:\n{json_err}")
                }
                server::ExtractError::MethodMismatch(_) => {
                    unreachable!("A method mismatch should not be possible here unless you've used a different handler (`N`) \
                        than the one whose method name was matched against earlier.")
                }
            })
            .with_failure_code(server::ErrorCode::InvalidParams)?,
    ))
}

pub(crate) struct Error {
    pub(crate) code: server::ErrorCode,
    pub(crate) error: anyhow::Error,
}

/// A trait to convert result types into the server result type, [`super::Result`].
trait LSPResult<T> {
    fn with_failure_code(self, code: server::ErrorCode) -> super::Result<T>;
}

impl<T, E: Into<anyhow::Error>> LSPResult<T> for core::result::Result<T, E> {
    fn with_failure_code(self, code: server::ErrorCode) -> super::Result<T> {
        self.map_err(|err| Error::new(err.into(), code))
    }
}

impl Error {
    pub(crate) fn new(err: anyhow::Error, code: server::ErrorCode) -> Self {
        Self { code, error: err }
    }
}

// Right now, we treat the error code as invisible data that won't
// be printed.
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
