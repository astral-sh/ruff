use std::process::Output;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use crossbeam::channel::{Receiver, RecvTimeoutError, SendTimeoutError, Sender, TrySendError};
use ruff_db::files::File;
use ruff_db::system::{CommandExecutor, System, SystemPathBuf};

use super::{MetadataTarget, Uv, unsupported_command_execution, uv_executable_error};
use crate::script::ScriptEnvironmentCacheKey;
use crate::{Db, ScriptSyncProgress};

/// Synchronizes standalone script environments with uv.
///
/// A project stores one service in its shared `ScriptEnvironments`, so every database snapshot uses
/// the same bounded request queue and workers. The queue applies backpressure to producers, while
/// the number of workers limits concurrent uv processes.
///
/// The service deliberately owns neither a database nor its `System`. A uv command can outlive the
/// check that scheduled it, while a database update must wait for all instances to be dropped.
/// Retaining the scheduling database could therefore prevent that update from completing. Workers
/// retain only the configured uv executable and a detached command executor.
///
/// The service only owns scheduling of `uv metadata` calls.
/// [`ScriptEnvironments`](crate::ScriptEnvironments) is the higher level abstraction that
/// application code should use.
pub(crate) struct UvSyncService {
    workers: OnceLock<std::io::Result<UvWorkerPool>>,

    /// Channel, where to send the background results to.
    results_sender: Sender<ScriptSyncResult>,

    /// Signals when new background results are available.
    ///
    /// This overlaps with `results_sender`, but the main difference is that it doesn't expose the
    /// sync result. The LSP and CLI use it as a wake up signal for when to call
    /// [`ScriptEnvironments::poll_sync`](crate::ScriptEnvironments::poll_sync).
    wake_sender: Sender<()>,
}

impl UvSyncService {
    pub(crate) fn new(results_sender: Sender<ScriptSyncResult>, wake_sender: Sender<()>) -> Self {
        Self {
            workers: OnceLock::new(),
            results_sender,
            wake_sender,
        }
    }

    /// Synchronizes one script and waits for its result.
    ///
    /// Blocks the calling thread, periodically checking for Salsa cancellation.
    ///
    /// This must not yield to Rayon. Another checking task could import this script and wait
    /// for its initialization, preventing the current task from resuming to finish it.
    /// The uv workers run on separate threads and can make progress while this thread waits.
    pub(crate) fn run_blocking(
        &self,
        db: &dyn Db,
        task: ScriptSyncTask,
    ) -> std::io::Result<Output> {
        let workers = self.worker_pool(db.system())?;

        // Dropping the guard during Salsa unwinding cancels a job that hasn't started yet.
        let (_cancellation_guard, cancellation) = UvJobCancellation::new();

        // Each job produces one result. Buffering that result lets the worker publish it without
        // waiting for this caller to be scheduled again.
        let (result_sender, result_receiver) = crossbeam::channel::bounded(1);

        let mut job = UvJob {
            task,
            mode: UvJobMode::Blocking {
                result_sender,
                cancellation,
            },
            span: tracing::Span::current(),
        };

        // Queue the job, checking for Salsa cancellation while waiting for capacity.
        loop {
            match workers.jobs.send_timeout(job, POLL_TIMEOUT) {
                Ok(()) => break,
                Err(SendTimeoutError::Timeout(pending)) => {
                    db.unwind_if_revision_cancelled();
                    job = pending;
                }
                Err(SendTimeoutError::Disconnected(_)) => return Err(worker_disconnected()),
            }
        }

        // Wait for the result.
        loop {
            match result_receiver.recv_timeout(POLL_TIMEOUT) {
                Ok(output) => return output,
                Err(RecvTimeoutError::Timeout) => {
                    db.unwind_if_revision_cancelled();
                }
                Err(RecvTimeoutError::Disconnected) => return Err(worker_disconnected()),
            }
        }
    }

    /// Submits one background synchronization.
    ///
    /// This blocks while the bounded worker queue is full, applying backpressure until a worker
    /// accepts another job.
    pub(crate) fn schedule_one(
        &self,
        system: &dyn System,
        task: ScriptSyncTask,
        progress: Option<Box<dyn ScriptSyncProgress>>,
    ) {
        let workers = match self.worker_pool(system) {
            Ok(workers) => workers,
            Err(error) => {
                self.publish_result(ScriptSyncResult {
                    task,
                    output: Err(error),
                    progress,
                });
                return;
            }
        };

        let path = task.request.path().to_path_buf();
        let job = UvJob {
            task,
            mode: UvJobMode::Background {
                result_sender: self.results_sender.clone(),
                wake_sender: self.wake_sender.clone(),
                progress,
            },
            span: tracing::debug_span!(
                "script_environment_sync",
                script = %path,
            ),
        };
        tracing::debug!("Queuing script synchronization for `{path}`");
        match workers.jobs.send(job) {
            Ok(()) => tracing::debug!("Queued script synchronization for `{path}`"),
            Err(error) => {
                let UvJob { task, mode, .. } = error.into_inner();
                match mode {
                    UvJobMode::Blocking { result_sender, .. } => {
                        let _ = result_sender.send(Err(worker_disconnected()));
                    }
                    UvJobMode::Background { progress, .. } => {
                        self.publish_result(ScriptSyncResult {
                            task,
                            output: Err(worker_disconnected()),
                            progress,
                        });
                    }
                }
            }
        }
    }

    fn publish_result(&self, result: ScriptSyncResult) {
        self.results_sender
            .send(result)
            .expect("the uv synchronization result receiver must remain connected");

        match self.wake_sender.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => {}
            Err(TrySendError::Disconnected(())) => {
                panic!("the uv synchronization wakeup receiver must remain connected");
            }
        }
    }

    fn worker_pool(&self, system: &dyn System) -> std::io::Result<&UvWorkerPool> {
        match self.workers.get_or_init(|| {
            let command_executor = system
                .command_executor()
                .ok_or_else(unsupported_command_execution)?;
            let uv = Uv::new(system).map_err(uv_executable_error)?;
            UvWorkerPool::new(command_executor, &uv)
        }) {
            Ok(workers) => Ok(workers),
            Err(error) => Err(std::io::Error::new(error.kind(), error.to_string())),
        }
    }
}

impl std::panic::RefUnwindSafe for UvSyncService {}

/// A standalone script environment that should be synchronized by uv.
#[derive(Debug)]
pub(crate) struct ScriptSyncTask {
    /// The script file
    pub(crate) file: File,
    pub(crate) request: ScriptSyncRequest,
}

impl ScriptSyncTask {
    pub(crate) fn new(
        file: File,
        path: SystemPathBuf,
        python: Option<SystemPathBuf>,
        cache_key: ScriptEnvironmentCacheKey,
    ) -> Self {
        Self {
            file,
            request: ScriptSyncRequest(Arc::new(ScriptSyncRequestData {
                path,
                python,
                cache_key,
            })),
        }
    }
}

/// The immutable inputs to one uv synchronization.
///
/// The entry state and worker retain cheap clones because a job can outlive the Salsa snapshot that
/// scheduled it.
#[derive(Clone, Debug)]
pub(crate) struct ScriptSyncRequest(Arc<ScriptSyncRequestData>);

impl ScriptSyncRequest {
    pub(crate) fn path(&self) -> &ruff_db::system::SystemPath {
        &self.0.path
    }

    /// The `--python` argument
    pub(crate) fn python(&self) -> Option<&ruff_db::system::SystemPath> {
        self.0.python.as_deref()
    }

    pub(crate) fn cache_key(&self) -> ScriptEnvironmentCacheKey {
        self.0.cache_key
    }
}

#[derive(Debug)]
struct ScriptSyncRequestData {
    path: SystemPathBuf,
    python: Option<SystemPathBuf>,
    cache_key: ScriptEnvironmentCacheKey,
}

/// The result of synchronizing a standalone script environment.
///
/// This owns the progress guard so progress remains active until the result is consumed.
pub(crate) struct ScriptSyncResult {
    pub(crate) task: ScriptSyncTask,
    pub(crate) output: std::io::Result<Output>,
    pub(crate) progress: Option<Box<dyn ScriptSyncProgress>>,
}

const MAX_UV_WORKERS: usize = 2;
const MAX_QUEUED_UV_TASKS: usize = 8;
/// Maximum time to block before checking for Salsa cancellation.
const POLL_TIMEOUT: Duration = Duration::from_millis(1);

/// A bounded worker pool for synchronizing standalone script environments with uv.
struct UvWorkerPool {
    /// Disconnects when the pool is dropped so workers abandon buffered jobs.
    ///
    /// This field precedes `jobs` so shutdown wins if dropping the pool makes both channels
    /// ready simultaneously.
    _shutdown: Sender<()>,

    /// Sender end of the pool <-> worker communication.
    ///
    /// Used to submit jobs to the workers.
    jobs: Sender<UvJob>,
}

impl UvWorkerPool {
    /// Creates worker threads using a detached command executor and resolved uv executable.
    fn new(command_executor: &dyn CommandExecutor, uv: &Uv) -> std::io::Result<Self> {
        let (jobs, job_receiver) = crossbeam::channel::bounded(MAX_QUEUED_UV_TASKS);
        let (shutdown, shutdown_receiver) = crossbeam::channel::bounded(0);

        let workers = ruff_db::max_parallelism().get().min(MAX_UV_WORKERS);

        tracing::debug!("Starting {workers} uv synchronization workers");

        for index in 0..workers {
            let worker = UvWorker {
                executor: command_executor.dyn_clone(),
                uv: uv.clone(),
                jobs: job_receiver.clone(),
                shutdown: shutdown_receiver.clone(),
            };

            let _ = std::thread::Builder::new()
                .name(format!("ty-uv-sync-{index}"))
                .spawn(move || worker.run())?;
        }

        Ok(Self {
            jobs,
            _shutdown: shutdown,
        })
    }
}

struct UvWorker {
    executor: Box<dyn CommandExecutor>,
    uv: Uv,

    /// Receiver end of the pool <-> worker channel.
    ///
    /// Used to retrieve jobs.
    jobs: Receiver<UvJob>,

    /// Channel used as a signal when the [`UvWorkerPool`] disconnects.
    ///
    /// When `jobs` disconnects, the receiver still yields all elements
    /// that are already queued, before returning `Disconnect`. We use this
    /// always-empty channel to be informed immediately if the worker pool disconnects,
    /// to avoid processing any unnecessary items.
    shutdown: Receiver<()>,
}

impl UvWorker {
    fn run(self) {
        loop {
            let job = crossbeam::channel::select_biased! {
                // The worker pool disconnected, exit immetiatley
                recv(self.shutdown) -> _ => return,
                recv(self.jobs) -> job => {
                    let Ok(job) = job else {
                        return;
                    };
                    job
                }
            };

            // Don't schedule jobs that have been cancelled in the meantime (salsa cancellation).
            if let UvJobMode::Blocking { cancellation, .. } = &job.mode
                && cancellation.is_cancelled()
            {
                tracing::debug!(
                    "Discarded cancelled script synchronization for `{}`",
                    job.task.request.path()
                );
                continue;
            }

            // Run the job
            let output = {
                let _span = job.span.enter();
                tracing::info!("Synchronizing script `{}`", job.task.request.path());

                let target = MetadataTarget::Script {
                    path: job.task.request.path(),
                    python: job.task.request.python(),
                };

                self.uv.execute(&*self.executor, target)
            };

            // Send the result
            let UvJob { task, mode, .. } = job;
            match mode {
                UvJobMode::Blocking { result_sender, .. } => {
                    // The receiver disappears when the blocking caller is cancelled.
                    let _ = result_sender.send(output);
                }
                UvJobMode::Background {
                    result_sender,
                    wake_sender,
                    progress,
                } => {
                    // The receiver disappears when the owning project is dropped.
                    if result_sender
                        .send(ScriptSyncResult {
                            task,
                            output,
                            progress,
                        })
                        .is_ok()
                    {
                        // Signal that there's a new result.
                        let _ = wake_sender.try_send(());
                    }
                }
            }
        }
    }
}

struct UvJob {
    task: ScriptSyncTask,
    mode: UvJobMode,
    span: tracing::Span,
}

enum UvJobMode {
    Blocking {
        /// Sender end of the channel that communicates with the thread waiting on this result (blocking).
        result_sender: Sender<std::io::Result<Output>>,
        /// Lets a worker discard a queued job after its blocking Salsa operation is cancelled.
        cancellation: UvJobCancellation,
    },

    /// The job runs as a background task.
    ///
    Background {
        /// The sender end of the channel communicating with the sync service.
        result_sender: Sender<ScriptSyncResult>,

        /// The wake signal that notifies that there's a new result to process.
        wake_sender: Sender<()>,
        progress: Option<Box<dyn ScriptSyncProgress>>,
    },
}

/// Cancellation token for a blocking uv operation.
///
/// The scheduling operation holds a [`UvJobCancellationGuard`] while the queued job holds this weak
/// token. Salsa cancellation unwinds the scheduling operation and drops the guard, allowing the
/// worker to discard the job without retaining the database or explicitly updating shared state.
/// An already running uv process is not interrupted.
struct UvJobCancellation(Weak<()>);

impl UvJobCancellation {
    fn new() -> (UvJobCancellationGuard, Self) {
        let guard = UvJobCancellationGuard(Arc::new(()));
        let cancellation = Self(Arc::downgrade(&guard.0));
        (guard, cancellation)
    }

    fn is_cancelled(&self) -> bool {
        self.0.upgrade().is_none()
    }
}

/// Keeps a blocking uv job active while its scheduling operation is running.
struct UvJobCancellationGuard(Arc<()>);

fn worker_disconnected() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "uv synchronization worker terminated unexpectedly",
    )
}
