use crate::server::schedule::Task;
use lsp_server as server;

mod notifications;
mod requests;
mod traits;

use notifications as notification;
use requests as request;
use traits::{BackgroundRequest, SyncNotification};

use super::Result;

/// A helper macro for [`select_task`] that builds a task from a successful request match.
/// It can take optional configuration in the form of `{ use <schedule> }` where `schedule` is
/// a constructor function of `Task`. This determines how the handler is scheduled.
macro_rules! handle_task {
    // If no configuration is provided, we create a background task by default.
    ($class: ty, $id: ident, $params: ident, $handle:ty) => {
        handle_task!($class, $id, $params, $handle { use background_thread })
    };
    // If we're building a sync task, the constructor takes slightly different
    // arguments, so this needs to be a special case.
    ($class: ty, $id: ident, $params: ident, $handle:ty { use local }) => {
        Task::local(move |session, notifier, responder| {
            let result = <$handle>::run(session, notifier, $params);
            <$handle as $class>::respond($id, result, &responder);
        })
    };
    // Otherwise, this is a builder for a background task of some `$schedule`.
    // We don't have access to the session here, so we need to create a 'builder' closure
    // around the inner task closure to take a snapshot when this task is dispatched.
    ($class: ty, $id: ident, $params: ident, $handle:ty { use $schedule:ident }) => {
        Task::$schedule(move |session| {
            // TODO(jane): we should log an error if we can't take a snapshot.
            let Some(snapshot) = session.take_snapshot(<$handle>::document_url(&$params)) else { return Box::new(|_, _| {}) };
            Box::new(move |notifier, responder| {
                let result = <$handle>::run_with_snapshot(snapshot, notifier, $params);
                <$handle as $class>::respond($id, result, &responder);
            })
        })
    };
}

/// Defines logic to route a server message sub-type to a series of handlers that share
/// a specific `$class` - in this case, [`traits::Request`] and [`traits::Notification`] are valid
/// handler classes. This macro generates the construction of each possible task based on the provided handler implementations.
/// The scheduling configuration for each task is also set here.
macro_rules! select_task {
    (match $req:ident as $class:ty { $($handle:ty$({ $($conf:tt)* })?),* $(,)? }) => {
        (move || {
            let build_err = |err| match err {
                json_err @ lsp_server::ExtractError::JsonError { .. } => {
                    let err: anyhow::Error = json_err.into();
                    anyhow::anyhow!("JSON parsing failure:\n{err}")
                },
                lsp_server::ExtractError::MethodMismatch(_) => {
                    unreachable!("A method mismatch should not be possible here, unless the `cast` implementation for this request has been changed.")
                }
            };
            match $req.method.as_str() {
                $(<$handle as $class>::METHOD => {
                    let (id, params) = <$handle as $class>::cast($req).map_err(build_err).with_failure_code(lsp_server::ErrorCode::ParseError)?;
                    Ok(handle_task!($class, id, params, $handle $({$($conf)*})?))
                }),*
                _ => Err(anyhow::anyhow!("No route found for {:?}", $req)).with_failure_code(lsp_server::ErrorCode::MethodNotFound)
            }
        })()
    };
}

/// Defines the `document_url` method for implementors of [`traits::Notification`] and [`traits::Request`],
/// given the parameter type used by the implementor.
macro_rules! define_document_url {
    ($params:ident: &$p:ty) => {
        fn document_url($params: &$p) -> &lsp_types::Url {
            &$params.text_document.uri
        }
    };
}

use define_document_url;

pub(super) fn request<'a>(req: server::Request) -> Task<'a> {
    let id = req.id.clone();
    select_task! {
        match req as traits::Request {
            request::CodeAction { use low_latency_thread },
            request::Diagnostic { use low_latency_thread },
            request::Format { use fmt_thread },
            request::FormatRange { use fmt_thread },
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing request: {err}");
        let result: Result<()> = Err(err);
        Task::immediate(id, result)
    })
}

pub(super) fn notification<'a>(notif: server::Notification) -> Task<'a> {
    select_task! {
        match notif as traits::Notification {
            notification::Cancel { use local },
            notification::DidOpen { use local },
            notification::DidChange { use local },
            notification::DidChangeConfiguration { use local },
            notification::DidChangeWorkspace { use local },
            notification::DidClose { use local },
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing notification: {err}");
        Task::nothing()
    })
}

pub(crate) struct Error {
    pub(crate) code: lsp_server::ErrorCode,
    pub(crate) error: anyhow::Error,
}

/// A trait to convert result types into the server result type, [`super::Result`].
trait LSPResult<T> {
    fn with_failure_code(self, code: lsp_server::ErrorCode) -> super::Result<T>;
}

impl<T> LSPResult<T> for anyhow::Result<T> {
    fn with_failure_code(self, code: server::ErrorCode) -> super::Result<T> {
        self.map_err(|err| Error::new(err, code))
    }
}

impl Error {
    pub(crate) fn new(err: anyhow::Error, code: lsp_server::ErrorCode) -> Self {
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
