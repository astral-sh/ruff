use std::num::NonZeroUsize;

use crate::server::{MAX_QUEUED_REQUESTS, api};
use crate::session::Session;
use crossbeam::channel::TrySendError;
use lsp_server::{ErrorCode, Request, ResponseError};

mod task;
mod thread;

use ruff_db::STACK_SIZE;

use self::{
    task::{BackgroundTaskBuilder, SyncTask},
    thread::ThreadPriority,
};
use crate::session::client::Client;
pub(super) use task::{BackgroundSchedule, Task};

/// The event loop thread is actually a secondary thread that we spawn from the
/// _actual_ main thread. This secondary thread has a larger stack size
/// than some OS defaults (Windows, for example) and is also designated as
/// high-priority.
pub(crate) fn spawn_main_loop(
    func: impl FnOnce() -> crate::Result<()> + Send + 'static,
) -> crate::Result<thread::JoinHandle<crate::Result<()>>> {
    const MAIN_THREAD_NAME: &str = "ty:main";
    Ok(
        thread::Builder::new(thread::ThreadPriority::LatencySensitive)
            .name(MAIN_THREAD_NAME.into())
            .stack_size(STACK_SIZE)
            .spawn(func)?,
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RequestAttempt {
    Initial,
    Retry,
}

pub(crate) struct Scheduler {
    fmt_pool: thread::Pool,
    background_pool: thread::Pool,
}

impl Scheduler {
    pub(super) fn new(worker_threads: NonZeroUsize) -> Self {
        const FMT_THREADS: usize = 1;
        Self {
            fmt_pool: thread::Pool::new(NonZeroUsize::try_from(FMT_THREADS).unwrap()),
            background_pool: thread::Pool::new(worker_threads),
        }
    }

    pub(super) fn dispatch_request(
        &mut self,
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
        match self.dispatch(task, session, client) {
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

    /// Dispatches a `task` by either running it as a blocking function or
    /// executing it on a background thread pool.
    pub(super) fn dispatch(
        &mut self,
        task: task::Task,
        session: &mut Session,
        client: &Client,
    ) -> Result<(), TrySendError<()>> {
        match task {
            Task::Sync(SyncTask { func }) => {
                func(session, client);
            }
            Task::Background(BackgroundTaskBuilder {
                schedule,
                builder: func,
                ..
            }) => {
                let (pool, priority) = match schedule {
                    BackgroundSchedule::Worker => {
                        (&mut self.background_pool, ThreadPriority::Worker)
                    }
                    BackgroundSchedule::LatencySensitive => {
                        (&mut self.background_pool, ThreadPriority::LatencySensitive)
                    }
                    BackgroundSchedule::Fmt => {
                        (&mut self.fmt_pool, ThreadPriority::LatencySensitive)
                    }
                };
                let job = pool.spawn(priority)?;
                let task = func(session);
                let client = client.clone();
                job.run(move || task(&client))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::time::Duration;

    use crossbeam::channel::{bounded, unbounded};
    use lsp_server::{ErrorCode, Request};
    use lsp_types::{CompletionRequest, Request as _, WorkspaceDiagnosticRequest};
    use ruff_db::system::TestSystem;
    use serde_json::json;

    use super::{RequestAttempt, Scheduler, ThreadPriority};
    use crate::capabilities::ResolvedClientCapabilities;
    use crate::server::{Action, Event, MAX_QUEUED_REQUESTS};
    use crate::session::{ClientName, InitializationOptions, client::Client};
    use crate::{PositionEncoding, Session};

    #[test]
    fn full_worker_queue_skips_retry_with_original_cancellation_error() -> anyhow::Result<()> {
        let mut scheduler = Scheduler::new(NonZeroUsize::MIN);
        let (started, ready) = bounded(0);
        let (release, wait) = bounded(0);
        let timeout = Duration::from_secs(10);
        let pool = &mut scheduler.background_pool;
        pool.spawn(ThreadPriority::Worker)?.run(move || {
            let _ = started.send(());
            let _ = wait.recv_timeout(timeout);
        })?;
        ready.recv_timeout(timeout)?;
        for _ in 1..MAX_QUEUED_REQUESTS {
            pool.spawn(ThreadPriority::Worker)?.run(|| {})?;
        }
        // Dropping a pending job leaves the last slot available.
        assert!(pool.spawn(ThreadPriority::Worker).is_ok());
        pool.spawn(ThreadPriority::Worker)?.run(|| {})?;

        let (sender, actions) = unbounded();
        let (outbox, _messages) = unbounded();
        let client = Client::new(sender, outbox);
        let mut session = Session::new(
            ResolvedClientCapabilities::empty(),
            PositionEncoding::default(),
            vec![],
            InitializationOptions::default(),
            Arc::new(TestSystem::default()),
            ClientName::Other,
            true,
        )?;

        let completion = |id| {
            Request::new(
                id,
                CompletionRequest::METHOD.to_string(),
                json!({
                    "textDocument": {"uri": "file:///file.py"}, "position": {"line": 0, "character": 0}
                }),
            )
        };
        let diagnostics = Request::new(
            3.into(),
            WorkspaceDiagnosticRequest::METHOD.to_string(),
            json!({"previousResultIds": []}),
        );
        for (request, attempt, expected, data) in [
            (
                completion(1.into()),
                RequestAttempt::Initial,
                ErrorCode::RequestFailed,
                None,
            ),
            (
                completion(2.into()),
                RequestAttempt::Retry,
                ErrorCode::ContentModified,
                None,
            ),
            (
                diagnostics,
                RequestAttempt::Retry,
                ErrorCode::ServerCancelled,
                Some(json!({"retriggerRequest": true})),
            ),
        ] {
            let id = request.id.clone();
            session
                .request_queue_mut()
                .incoming_mut()
                .register(request.id.clone(), request.method.clone());
            scheduler.dispatch_request(request, &mut session, &client, attempt)?;
            let Event::Action(Action::SendResponse(response)) = actions.try_recv()? else {
                anyhow::bail!("expected a response for request {id}");
            };
            assert_eq!(response.id, id);
            let error = response.response_result.unwrap_err();
            assert_eq!(error.code, expected as i32);
            assert_eq!(error.data, data);
        }
        release.send(())?;
        Ok(())
    }
}
