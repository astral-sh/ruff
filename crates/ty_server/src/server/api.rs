use crate::server::schedule::Task;
use crate::session::Session;
use crate::system::{AnySystemPath, url_to_any_system_path};
use anyhow::anyhow;
use lsp_server as server;
use lsp_server::RequestId;
use lsp_types::notification::Notification;
use ruff_db::panic::PanicError;
use std::panic::UnwindSafe;

mod diagnostics;
mod notifications;
mod requests;
mod traits;

use self::traits::{NotificationHandler, RequestHandler};
use super::{Result, client::Responder, schedule::BackgroundSchedule};

pub(super) fn request(req: server::Request) -> Task {
    let id = req.id.clone();

    match req.method.as_str() {
        requests::DocumentDiagnosticRequestHandler::METHOD => background_request_task::<
            requests::DocumentDiagnosticRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::GotoTypeDefinitionRequestHandler::METHOD => background_request_task::<
            requests::GotoTypeDefinitionRequestHandler,
        >(
            req, BackgroundSchedule::Worker
        ),
        requests::HoverRequestHandler::METHOD => background_request_task::<
            requests::HoverRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::InlayHintRequestHandler::METHOD => background_request_task::<
            requests::InlayHintRequestHandler,
        >(req, BackgroundSchedule::Worker),
        requests::CompletionRequestHandler::METHOD => background_request_task::<
            requests::CompletionRequestHandler,
        >(
            req, BackgroundSchedule::LatencySensitive
        ),

        method => {
            tracing::warn!("Received request {method} which does not have a handler");
            let result: Result<()> = Err(Error::new(
                anyhow!("Unknown request"),
                server::ErrorCode::MethodNotFound,
            ));
            return Task::immediate(id, result);
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing request with ID {id}: {err}");
        show_err_msg!(
            "ty failed to handle a request from the editor. Check the logs for more details."
        );
        let result: Result<()> = Err(err);
        Task::immediate(id, result)
    })
}

pub(super) fn notification(notif: server::Notification) -> Task {
    match notif.method.as_str() {
        notifications::DidCloseTextDocumentHandler::METHOD => {
            local_notification_task::<notifications::DidCloseTextDocumentHandler>(notif)
        }
        notifications::DidOpenTextDocumentHandler::METHOD => {
            local_notification_task::<notifications::DidOpenTextDocumentHandler>(notif)
        }
        notifications::DidChangeTextDocumentHandler::METHOD => {
            local_notification_task::<notifications::DidChangeTextDocumentHandler>(notif)
        }
        notifications::DidOpenNotebookHandler::METHOD => {
            local_notification_task::<notifications::DidOpenNotebookHandler>(notif)
        }
        notifications::DidCloseNotebookHandler::METHOD => {
            local_notification_task::<notifications::DidCloseNotebookHandler>(notif)
        }
        notifications::DidChangeWatchedFiles::METHOD => {
            local_notification_task::<notifications::DidChangeWatchedFiles>(notif)
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
        show_err_msg!(
            "ty failed to handle a notification from the editor. Check the logs for more details."
        );
        Task::nothing()
    })
}

fn _local_request_task<R: traits::SyncRequestHandler>(req: server::Request) -> super::Result<Task>
where
    <<R as RequestHandler>::RequestType as lsp_types::request::Request>::Params: UnwindSafe,
{
    let (id, params) = cast_request::<R>(req)?;
    Ok(Task::local(|session, notifier, requester, responder| {
        let _span = tracing::debug_span!("request", %id, method = R::METHOD).entered();
        let result = R::run(session, notifier, requester, params);
        respond::<R>(id, result, &responder);
    }))
}

fn background_request_task<R: traits::BackgroundDocumentRequestHandler>(
    req: server::Request,
    schedule: BackgroundSchedule,
) -> super::Result<Task>
where
    <<R as RequestHandler>::RequestType as lsp_types::request::Request>::Params: UnwindSafe,
{
    let (id, params) = cast_request::<R>(req)?;
    Ok(Task::background(schedule, move |session: &Session| {
        let url = R::document_url(&params).into_owned();

        let Ok(path) = url_to_any_system_path(&url) else {
            tracing::warn!("Ignoring request for invalid `{url}`");
            return Box::new(|_, _| {});
        };

        let db = match &path {
            AnySystemPath::System(path) => match session.project_db_for_path(path.as_std_path()) {
                Some(db) => db.clone(),
                None => session.default_project_db().clone(),
            },
            AnySystemPath::SystemVirtual(_) => session.default_project_db().clone(),
        };

        let Some(snapshot) = session.take_snapshot(url) else {
            tracing::warn!("Ignoring request because snapshot for path `{path:?}` doesn't exist.");
            return Box::new(|_, _| {});
        };

        Box::new(move |notifier, responder| {
            let _span = tracing::debug_span!("request", %id, method = R::METHOD).entered();
            let result = ruff_db::panic::catch_unwind(|| {
                R::run_with_snapshot(&db, snapshot, notifier, params)
            });

            if let Some(response) = request_result_to_response(&id, &responder, result) {
                respond::<R>(id, response, &responder);
            }
        })
    }))
}

fn request_result_to_response<R>(
    id: &RequestId,
    responder: &Responder,
    result: std::result::Result<Result<R>, PanicError>,
) -> Option<Result<R>> {
    match result {
        Ok(response) => Some(response),
        Err(error) => {
            if error.payload.downcast_ref::<salsa::Cancelled>().is_some() {
                // Request was cancelled by Salsa. TODO: Retry
                respond_silent_error(
                    id.clone(),
                    responder,
                    Error {
                        code: lsp_server::ErrorCode::ContentModified,
                        error: anyhow!("content modified"),
                    },
                );
                None
            } else {
                let message = format!("request handler {error}");

                Some(Err(Error {
                    code: lsp_server::ErrorCode::InternalError,
                    error: anyhow!(message),
                }))
            }
        }
    }
}

fn local_notification_task<N: traits::SyncNotificationHandler>(
    notif: server::Notification,
) -> super::Result<Task> {
    let (id, params) = cast_notification::<N>(notif)?;
    Ok(Task::local(move |session, notifier, requester, _| {
        let _span = tracing::debug_span!("notification", method = N::METHOD).entered();
        if let Err(err) = N::run(session, notifier, requester, params) {
            tracing::error!("An error occurred while running {id}: {err}");
            show_err_msg!("ty encountered a problem. Check the logs for more details.");
        }
    }))
}

#[expect(dead_code)]
fn background_notification_thread<N>(
    req: server::Notification,
    schedule: BackgroundSchedule,
) -> super::Result<Task>
where
    N: traits::BackgroundDocumentNotificationHandler,
    <<N as NotificationHandler>::NotificationType as lsp_types::notification::Notification>::Params:
        UnwindSafe,
{
    let (id, params) = cast_notification::<N>(req)?;
    Ok(Task::background(schedule, move |session: &Session| {
        let url = N::document_url(&params);
        let Some(snapshot) = session.take_snapshot((*url).clone()) else {
            tracing::debug!(
                "Ignoring notification because snapshot for url `{url}` doesn't exist."
            );
            return Box::new(|_, _| {});
        };
        Box::new(move |notifier, _| {
            let _span = tracing::debug_span!("notification", method = N::METHOD).entered();

            let result = match ruff_db::panic::catch_unwind(|| {
                N::run_with_snapshot(snapshot, notifier, params)
            }) {
                Ok(result) => result,
                Err(panic) => {
                    tracing::error!("An error occurred while running {id}: {panic}");
                    show_err_msg!("ty encountered a panic. Check the logs for more details.");
                    return;
                }
            };

            if let Err(err) = result {
                tracing::error!("An error occurred while running {id}: {err}");
                show_err_msg!("ty encountered a problem. Check the logs for more details.");
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
) -> super::Result<(
    server::RequestId,
    <<Req as RequestHandler>::RequestType as lsp_types::request::Request>::Params,
)>
where
    Req: traits::RequestHandler,
    <<Req as RequestHandler>::RequestType as lsp_types::request::Request>::Params: UnwindSafe,
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
        .with_failure_code(server::ErrorCode::InternalError)
}

/// Sends back a response to the server using a [`Responder`].
fn respond<Req>(
    id: server::RequestId,
    result: crate::server::Result<
        <<Req as traits::RequestHandler>::RequestType as lsp_types::request::Request>::Result,
    >,
    responder: &Responder,
) where
    Req: traits::RequestHandler,
{
    if let Err(err) = &result {
        tracing::error!("An error occurred with request ID {id}: {err}");
        show_err_msg!("ty encountered a problem. Check the logs for more details.");
    }
    if let Err(err) = responder.respond(id, result) {
        tracing::error!("Failed to send response: {err}");
    }
}

/// Sends back an error response to the server using a [`Responder`] without showing a warning
/// to the user.
fn respond_silent_error(id: server::RequestId, responder: &Responder, error: Error) {
    if let Err(err) = responder.respond::<()>(id, Err(error)) {
        tracing::error!("Failed to send response: {err}");
    }
}

/// Tries to cast a serialized request from the server into
/// a parameter type for a specific request handler.
fn cast_notification<N>(
    notification: server::Notification,
) -> super::Result<
    (
        &'static str,
        <<N as traits::NotificationHandler>::NotificationType as lsp_types::notification::Notification>::Params,
    )> where
    N: traits::NotificationHandler,
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
            .with_failure_code(server::ErrorCode::InternalError)?,
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
