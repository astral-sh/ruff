use std::num::NonZeroUsize;

use crate::session::Session;

mod task;
mod thread;

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
    // Override OS defaults to avoid stack overflows on platforms with low stack size defaults.
    const MAIN_THREAD_STACK_SIZE: usize = 2 * 1024 * 1024;
    const MAIN_THREAD_NAME: &str = "ty:main";
    Ok(
        thread::Builder::new(thread::ThreadPriority::LatencySensitive)
            .name(MAIN_THREAD_NAME.into())
            .stack_size(MAIN_THREAD_STACK_SIZE)
            .spawn(func)?,
    )
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

    /// Dispatches a `task` by either running it as a blocking function or
    /// executing it on a background thread pool.
    pub(super) fn dispatch(&mut self, task: task::Task, session: &mut Session, client: Client) {
        match task {
            Task::Sync(SyncTask { func }) => {
                func(session, &client);
            }
            Task::Background(BackgroundTaskBuilder {
                schedule,
                builder: func,
            }) => {
                let static_func = func(session);
                let task = move || static_func(&client);
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
