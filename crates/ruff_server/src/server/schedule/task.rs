use lsp_server::RequestId;
use serde::Serialize;

use crate::{
    server::client::{Notifier, Requester, Responder},
    session::Session,
};

type LocalFn<'s> = Box<dyn FnOnce(&mut Session, Notifier, &mut Requester, Responder) + 's>;

type BackgroundFn = Box<dyn FnOnce(Notifier, Responder) + Send + 'static>;

type BackgroundFnBuilder<'s> = Box<dyn FnOnce(&Session) -> BackgroundFn + 's>;

/// Describes how the task should be run.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::server) enum BackgroundSchedule {
    /// The task should be run on the background thread designated
    /// for formatting actions. This is a high priority thread.
    Fmt,
    /// The task should be run on the general high-priority background
    /// thread.
    LatencySensitive,
    /// The task should be run on a regular-priority background thread.
    #[default]
    Worker,
}

/// A [`Task`] is a future that has not yet started, and it is the job of
/// the [`super::Scheduler`] to make that happen, via [`super::Scheduler::dispatch`].
/// A task can either run on the main thread (in other words, the same thread as the
/// scheduler) or it can run in a background thread. The main difference between
/// the two is that background threads only have a read-only snapshot of the session,
/// while local tasks have exclusive access and can modify it as they please. Keep in mind that
/// local tasks will **block** the main event loop, so only use local tasks if you **need**
/// mutable state access or you need the absolute lowest latency possible.
pub(in crate::server) enum Task<'s> {
    Background(BackgroundTaskBuilder<'s>),
    Sync(SyncTask<'s>),
}

// The reason why this isn't just a 'static background closure
// is because we need to take a snapshot of the session before sending
// this task to the background, and the inner closure can't take the session
// as an immutable reference since it's used mutably elsewhere. So instead,
// a background task is built using an outer closure that borrows the session to take a snapshot,
// that the inner closure can capture. This builder closure has a lifetime linked to the scheduler.
// When the task is dispatched, the scheduler runs the synchronous builder, which takes the session
// as a reference, to create the inner 'static closure. That closure is then moved to a background task pool.
pub(in crate::server) struct BackgroundTaskBuilder<'s> {
    pub(super) schedule: BackgroundSchedule,
    pub(super) builder: BackgroundFnBuilder<'s>,
}

pub(in crate::server) struct SyncTask<'s> {
    pub(super) func: LocalFn<'s>,
}

impl<'s> Task<'s> {
    /// Creates a new background task.
    pub(crate) fn background(
        schedule: BackgroundSchedule,
        func: impl FnOnce(&Session) -> Box<dyn FnOnce(Notifier, Responder) + Send + 'static> + 's,
    ) -> Self {
        Self::Background(BackgroundTaskBuilder {
            schedule,
            builder: Box::new(func),
        })
    }
    /// Creates a new local task.
    pub(crate) fn local(
        func: impl FnOnce(&mut Session, Notifier, &mut Requester, Responder) + 's,
    ) -> Self {
        Self::Sync(SyncTask {
            func: Box::new(func),
        })
    }
    /// Creates a local task that immediately
    /// responds with the provided `request`.
    pub(crate) fn immediate<R>(id: RequestId, result: crate::server::Result<R>) -> Self
    where
        R: Serialize + Send + 'static,
    {
        Self::local(move |_, _, _, responder| {
            if let Err(err) = responder.respond(id, result) {
                tracing::error!("Unable to send immediate response: {err}");
            }
        })
    }

    /// Creates a local task that does nothing.
    pub(crate) fn nothing() -> Self {
        Self::local(move |_, _, _, _| {})
    }
}
