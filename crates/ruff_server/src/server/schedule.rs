use crossbeam::channel::Sender;

use crate::session::Session;

mod task;
mod thread;

pub(super) use task::Task;

use self::{
    task::{BackgroundSchedule, BackgroundTaskBuilder, SyncTask},
    thread::ThreadPriority,
};

use super::client::Client;

/// The main thread is actually a secondary thread that we spawn from the
/// _actual_ main thread. This secondary thread has a larger stack size
/// than some OS defaults (Windows, for example) and is also designated as
/// high-priority.
pub(crate) fn main_thread(
    func: impl FnOnce() -> crate::Result<()> + Send + 'static,
) -> crate::Result<thread::JoinHandle<crate::Result<()>>> {
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
        thread_count: usize,
        sender: &Sender<lsp_server::Message>,
    ) -> Self {
        Self {
            session,
            fmt_pool: thread::Pool::new(1),
            background_pool: thread::Pool::new(thread_count),
            client: Client::new(sender),
        }
    }

    /// This is executed lazily when a new message is received, but it will run before
    /// any message handling logic.
    pub(super) fn process_events(&mut self) {
        // TODO: figure out how to notify client to run a diagnostic refresh.
        // We might need to push diagnostics with notifications to begin with.
        self.session.update_configuration_files();
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
