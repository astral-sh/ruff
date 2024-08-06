use crate::{server::schedule::Task, session::Session, system::url_to_system_path};
use lsp_server as server;

mod diagnostics;
mod notifications;
mod requests;
mod traits;

use notifications as notification;
use red_knot_workspace::db::RootDatabase;
use requests as request;

use self::traits::{NotificationHandler, RequestHandler};

use super::{client::Responder, schedule::BackgroundSchedule, Result};

pub(super) fn request<'a>(req: server::Request) -> Task<'a> {
    let id = req.id.clone();

    match req.method.as_str() {
        request::DocumentDiagnosticRequestHandler::METHOD => {
            background_request_task::<request::DocumentDiagnosticRequestHandler>(
                req,
                BackgroundSchedule::LatencySensitive,
            )
        }
        method => {
            tracing::warn!("Received request {method} which does not have a handler");
            return Task::nothing();
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing request with ID {id}: {err}");
        show_err_msg!(
            "Ruff failed to handle a request from the editor. Check the logs for more details."
        );
        let result: Result<()> = Err(err);
        Task::immediate(id, result)
    })
}

pub(super) fn notification<'a>(notif: server::Notification) -> Task<'a> {
    match notif.method.as_str() {
        notification::DidCloseTextDocumentHandler::METHOD => local_notification_task::<notification::DidCloseTextDocumentHandler>(notif),
        notification::DidOpenTextDocumentHandler::METHOD => local_notification_task::<notification::DidOpenTextDocumentHandler>(notif),
        notification::DidOpenNotebookHandler::METHOD => {
            local_notification_task::<notification::DidOpenNotebookHandler>(notif)
        }
        notification::DidCloseNotebookHandler::METHOD => {
            local_notification_task::<notification::DidCloseNotebookHandler>(notif)
        }
        notification::SetTraceHandler::METHOD => {
            local_notification_task::<notification::SetTraceHandler>(notif)
        }
        method => {
            tracing::warn!("Received notification {method} which does not have a handler.");
            return Task::nothing();
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing notification: {err}");
        show_err_msg!("Ruff failed to handle a notification from the editor. Check the logs for more details.");
        Task::nothing()
    })
}

fn _local_request_task<'a, R: traits::SyncRequestHandler>(
    req: server::Request,
) -> super::Result<Task<'a>> {
    let (id, params) = cast_request::<R>(req)?;
    Ok(Task::local(|session, notifier, requester, responder| {
        let result = R::run(session, notifier, requester, params);
        respond::<R>(id, result, &responder);
    }))
}

fn background_request_task<'a, R: traits::BackgroundDocumentRequestHandler>(
    req: server::Request,
    schedule: BackgroundSchedule,
) -> super::Result<Task<'a>> {
    let (id, params) = cast_request::<R>(req)?;
    Ok(Task::background(schedule, move |session: &Session| {
        let url = R::document_url(&params).into_owned();

        let Ok(path) = url_to_system_path(&url) else {
            return Box::new(|_, _| {});
        };
        let db = session
            .workspace_db_for_path(path.as_std_path())
            .map(RootDatabase::snapshot);

        let Some(snapshot) = session.take_snapshot(url) else {
            return Box::new(|_, _| {});
        };

        Box::new(move |notifier, responder| {
            let result = R::run_with_snapshot(snapshot, db, notifier, params);
            respond::<R>(id, result, &responder);
        })
    }))
}

fn local_notification_task<'a, N: traits::SyncNotificationHandler>(
    notif: server::Notification,
) -> super::Result<Task<'a>> {
    let (id, params) = cast_notification::<N>(notif)?;
    Ok(Task::local(move |session, notifier, requester, _| {
        if let Err(err) = N::run(session, notifier, requester, params) {
            tracing::error!("An error occurred while running {id}: {err}");
            show_err_msg!("Ruff encountered a problem. Check the logs for more details.");
        }
    }))
}

#[allow(dead_code)]
fn background_notification_thread<'a, N: traits::BackgroundDocumentNotificationHandler>(
    req: server::Notification,
    schedule: BackgroundSchedule,
) -> super::Result<Task<'a>> {
    let (id, params) = cast_notification::<N>(req)?;
    Ok(Task::background(schedule, move |session: &Session| {
        // TODO(jane): we should log an error if we can't take a snapshot.
        let Some(snapshot) = session.take_snapshot(N::document_url(&params).into_owned()) else {
            return Box::new(|_, _| {});
        };
        Box::new(move |notifier, _| {
            if let Err(err) = N::run_with_snapshot(snapshot, notifier, params) {
                tracing::error!("An error occurred while running {id}: {err}");
                show_err_msg!("Ruff encountered a problem. Check the logs for more details.");
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
        tracing::error!("An error occurred with result ID {id}: {err}");
        show_err_msg!("Ruff encountered a problem. Check the logs for more details.");
    }
    if let Err(err) = responder.respond(id, result) {
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
)> where N: traits::NotificationHandler{
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
