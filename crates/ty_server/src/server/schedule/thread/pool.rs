// +------------------------------------------------------------+
// | Code adopted from:                                         |
// | Repository: https://github.com/rust-lang/rust-analyzer.git |
// | File: `crates/stdx/src/thread/pool.rs`                     |
// | Commit: 03b3cb6be9f21c082f4206b35c7fe7f291c94eaa           |
// +------------------------------------------------------------+
//! [`Pool`] implements a basic custom thread pool
//! inspired by the [`threadpool` crate](http://docs.rs/threadpool).
//! When you spawn a task you specify a thread priority
//! so the pool can schedule it to run on a thread with that priority.
//! rust-analyzer uses this to prioritize work based on latency requirements.
//!
//! The thread pool is implemented entirely using
//! the threading utilities in [`crate::server::schedule::thread`].

use crossbeam::channel::{Receiver, Sender};
use std::panic::AssertUnwindSafe;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use super::{Builder, JoinHandle, ThreadPriority};

pub(crate) struct Pool {
    // `_handles` is never read: the field is present
    // only for its `Drop` impl.

    // The worker threads exit once the channel closes;
    // make sure to keep `job_sender` above `handles`
    // so that the channel is actually closed
    // before we join the worker threads!
    job_sender: Sender<Job>,
    _handles: Vec<JoinHandle>,
    extant_tasks: Arc<AtomicUsize>,
}

struct Job {
    requested_priority: ThreadPriority,
    f: Box<dyn FnOnce() + Send + 'static>,
}

impl Pool {
    pub(crate) fn new(threads: NonZeroUsize) -> Pool {
        // Override OS defaults to avoid stack overflows on platforms with low stack size defaults.
        const STACK_SIZE: usize = 2 * 1024 * 1024;
        const INITIAL_PRIORITY: ThreadPriority = ThreadPriority::Worker;

        let threads = usize::from(threads);

        let (job_sender, job_receiver) = crossbeam::channel::bounded(std::cmp::min(threads * 2, 4));
        let extant_tasks = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::with_capacity(threads);
        for i in 0..threads {
            let handle = Builder::new(INITIAL_PRIORITY)
                .stack_size(STACK_SIZE)
                .name(format!("ty:worker:{i}"))
                .spawn({
                    let extant_tasks = Arc::clone(&extant_tasks);
                    let job_receiver: Receiver<Job> = job_receiver.clone();
                    move || {
                        let mut current_priority = INITIAL_PRIORITY;
                        for job in job_receiver {
                            if job.requested_priority != current_priority {
                                job.requested_priority.apply_to_current_thread();
                                current_priority = job.requested_priority;
                            }
                            extant_tasks.fetch_add(1, Ordering::SeqCst);

                            // SAFETY: it's safe to assume that `job.f` is unwind safe because we always
                            // abort the process if it panics.
                            // Panicking here ensures that we don't swallow errors and is the same as
                            // what rayon does.
                            // Any recovery should be implemented outside the thread pool (e.g. when
                            // dispatching requests/notifications etc).
                            if let Err(error) = std::panic::catch_unwind(AssertUnwindSafe(job.f)) {
                                if let Some(msg) = error.downcast_ref::<String>() {
                                    tracing::error!("Worker thread panicked with: {msg}; aborting");
                                } else if let Some(msg) = error.downcast_ref::<&str>() {
                                    tracing::error!("Worker thread panicked with: {msg}; aborting");
                                } else if let Some(cancelled) =
                                    error.downcast_ref::<salsa::Cancelled>()
                                {
                                    tracing::error!(
                                        "Worker thread got cancelled: {cancelled}; aborting"
                                    );
                                } else {
                                    tracing::error!(
                                        "Worker thread panicked with: {error:?}; aborting"
                                    );
                                }

                                std::process::abort();
                            }

                            extant_tasks.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                })
                .expect("failed to spawn thread");

            handles.push(handle);
        }

        Pool {
            _handles: handles,
            extant_tasks,
            job_sender,
        }
    }

    pub(crate) fn spawn<F>(&self, priority: ThreadPriority, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let f = Box::new(move || {
            if cfg!(debug_assertions) {
                priority.assert_is_used_on_current_thread();
            }
            f();
        });

        let job = Job {
            requested_priority: priority,
            f,
        };
        self.job_sender.send(job).unwrap();
    }

    #[expect(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.extant_tasks.load(Ordering::SeqCst)
    }
}
