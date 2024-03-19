use std::num::NonZeroUsize;

use crossbeam::channel::Sender;

use crate::session::Session;

mod task;
mod thread;

pub(super) use task::{BackgroundSchedule, Task};

use self::{
    task::{BackgroundTaskBuilder, SyncTask},
    thread::ThreadPriority,
};

use super::client::Client;

/// The event loop thread is actually a secondary thread that we spawn from the
/// _actual_ main thread. This secondary thread has a larger stack size
/// than some OS defaults (Windows, for example) and is also designated as
/// high-priority.
pub(crate) fn event_loop_thread(
    func: impl FnOnce() -> crate::Result<()> + Send + 'static,
) -> crate::Result<thread::JoinHandle<crate::Result<()>>> {
    // Override OS defaults to avoid stack overflows on platforms with low stack size defaults.
    const MAIN_THREAD_STACK_SIZE: usize = 2 * 1024 * 1024;
    const MAIN_THREAD_NAME: &str = "ruff:main";
    Ok(
        thread::Builder::new(thread::ThreadPriority::LatencySensitive)
            .name(MAIN_THREAD_NAME.into())
            .stack_size(MAIN_THREAD_STACK_SIZE)
            .spawn(func)?,
    )
}

pub(crate) struct Scheduler {
    session: Session,
    client: Client,
    fmt_pool: thread::Pool,
    background_pool: thread::Pool,
}

impl Scheduler {
    pub(super) fn new(
        session: Session,
        worker_threads: NonZeroUsize,
        sender: &Sender<lsp_server::Message>,
    ) -> Self {
        const FMT_THREADS: usize = 1;
        Self {
            session,
            fmt_pool: thread::Pool::new(NonZeroUsize::try_from(FMT_THREADS).unwrap()),
            background_pool: thread::Pool::new(worker_threads),
            client: Client::new(sender),
        }
    }

    /// Dispatches a `task` by either running it as a blocking function or
    /// executing it on a background thread pool.
    pub(super) fn dispatch<'s>(&'s mut self, task: task::Task<'s>) {
        match task {
            Task::Sync(SyncTask { func }) => {
                func(
                    &mut self.session,
                    self.client.notifier(),
                    self.client.responder(),
                );
            }
            Task::Background(BackgroundTaskBuilder {
                schedule,
                builder: func,
            }) => {
                let static_func = func(&self.session);
                let notifier = self.client.notifier();
                let responder = self.client.responder();
                let task = move || static_func(notifier, responder);
                match schedule {
                    BackgroundSchedule::Worker => {
                        self.background_pool.spawn(ThreadPriority::Worker, task);
                    }
                    BackgroundSchedule::LatencySensitive => self
                        .background_pool
                        .spawn(ThreadPriority::LatencySensitive, task),
                    BackgroundSchedule::Fmt => {
                        self.fmt_pool.spawn(ThreadPriority::LatencySensitive, task);
                    }
                }
            }
        }
    }
}
