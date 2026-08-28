use std::process::Output;
use std::sync::{Arc, OnceLock};

use crossbeam::channel::{Receiver, Sender, TrySendError};
use ruff_db::cancellation::CancellationToken;
use ruff_db::files::File;
use ruff_db::system::{CommandExecutor, System, SystemPathBuf};

use super::command::unsupported_command_execution;
use super::{MetadataTarget, ScriptEnvironmentCacheKey, Uv, uv_executable_error};
use crate::UvSyncProgress;

/// Runs workspace and standalone-script metadata requests with uv.
///
/// A project stores one service in its shared `UvEnvironments`, so every database snapshot uses
/// the same request queue and workers. The queue has no fixed capacity, so submitting work does
/// not wait for uv. The number of workers still limits concurrent uv processes.
///
/// [`UvEnvironments`](crate::UvEnvironments) submits at most one job per project or script at a
/// time, including jobs whose results have not yet been consumed. It retains only the latest
/// follow-up and submits it after consuming the previous result. Both request and result queues
/// are therefore bounded by the number of projects and scripts, not the number of changes.
/// Superseded jobs are cancelled before execution when possible. Running uv processes are not
/// interrupted, and cancelled jobs still return a result so their replacements can be scheduled.
///
/// The service deliberately owns neither a database nor its `System`. Workers stay alive between
/// jobs, while the host must be able to apply file changes and completed uv results. Salsa waits
/// for all database snapshots to be dropped before updating an input. If a worker retained a
/// snapshot, even an idle worker would block those updates. Workers retain only the configured uv
/// executable and a detached command executor.
///
/// The service only owns scheduling of `uv metadata` calls.
/// [`UvEnvironments`](crate::UvEnvironments) is the higher level abstraction that
/// application code should use.
pub(crate) struct UvMetadataService {
    workers: OnceLock<std::io::Result<UvWorkerPool>>,

    /// Channel, where to send the background results to.
    results_sender: Sender<UvMetadataResult>,

    /// Signals when new background results are available.
    ///
    /// This overlaps with `results_sender`, but the main difference is that it doesn't expose the
    /// sync result. The LSP and CLI use it as a wake up signal for when to call
    /// [`UvEnvironments::poll_sync`](crate::UvEnvironments::poll_sync).
    wake_sender: Sender<()>,
}

impl UvMetadataService {
    pub(crate) fn new(results_sender: Sender<UvMetadataResult>, wake_sender: Sender<()>) -> Self {
        Self {
            workers: OnceLock::new(),
            results_sender,
            wake_sender,
        }
    }

    /// Submits one background synchronization.
    ///
    /// Submission does not wait for queue space. Cancellation skips a job that has not started,
    /// but still produces a result and wakeup. It does not interrupt a running uv process.
    pub(crate) fn schedule_one(
        &self,
        system: &dyn System,
        task: UvSyncTask,
        cancellation: CancellationToken,
        progress: Option<Box<dyn UvSyncProgress>>,
    ) {
        let workers = match self.worker_pool(system) {
            Ok(workers) => workers,
            Err(error) => {
                self.publish_result(UvMetadataResult {
                    task,
                    output: Some(Err(error)),
                    progress,
                });
                return;
            }
        };

        let (path, description) = match &task {
            UvSyncTask::Workspace(path) => (path.as_path(), "workspace metadata"),
            UvSyncTask::Script(task) => (task.request.path(), "script synchronization"),
        };
        let span = tracing::debug_span!("uv_metadata", path = %path);
        tracing::debug!("Queuing {description} for `{path}`");

        let job = UvJob {
            task,
            cancellation,
            result_sender: self.results_sender.clone(),
            wake_sender: self.wake_sender.clone(),
            progress,
            span,
        };
        if let Err(error) = workers.jobs.send(job) {
            let job = error.into_inner();
            self.publish_result(UvMetadataResult {
                task: job.task,
                output: Some(Err(worker_disconnected())),
                progress: job.progress,
            });
        }
    }

    fn publish_result(&self, result: UvMetadataResult) {
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

impl std::panic::RefUnwindSafe for UvMetadataService {}

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
/// The entry state and worker retain cheap clones of the request. These inputs are owned so jobs
/// can outlive the host operation that scheduled them without retaining its database.
#[derive(Clone, Debug)]
pub(crate) struct ScriptSyncRequest(Arc<ScriptSyncRequestData>);

impl ScriptSyncRequest {
    pub(crate) fn path(&self) -> &ruff_db::system::SystemPath {
        &self.0.path
    }

    fn metadata_target(&self) -> MetadataTarget<'_> {
        MetadataTarget::Script {
            path: &self.0.path,
            python: self.0.python.as_deref(),
        }
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

/// Identifies a background workspace or script request.
#[derive(Debug)]
pub(crate) enum UvSyncTask {
    Workspace(SystemPathBuf),
    Script(ScriptSyncTask),
}

impl UvSyncTask {
    fn metadata_target(&self) -> MetadataTarget<'_> {
        match self {
            Self::Workspace(path) => MetadataTarget::Workspace(path),
            Self::Script(task) => task.request.metadata_target(),
        }
    }
}

/// The result of a background uv metadata request.
///
/// This keeps the progress reporter alive until the result is consumed or rescheduled.
pub(crate) struct UvMetadataResult {
    pub(crate) task: UvSyncTask,
    /// `None` if the worker skipped this request because it was cancelled before execution.
    pub(crate) output: Option<std::io::Result<Output>>,
    pub(crate) progress: Option<Box<dyn UvSyncProgress>>,
}

/// Runs a limited number of workspace and script metadata commands concurrently.
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
        let (jobs, job_receiver) = crossbeam::channel::unbounded();
        let (shutdown, shutdown_receiver) = crossbeam::channel::bounded(0);

        let workers = ruff_db::max_parallelism().get().div_ceil(4);

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
            let mut job = crossbeam::channel::select_biased! {
                // The worker pool disconnected, exit immetiatley
                recv(self.shutdown) -> _ => return,
                recv(self.jobs) -> job => {
                    let Ok(job) = job else {
                        return;
                    };
                    job
                }
            };

            let _span = job.span.enter();
            let output = if job.cancellation.is_cancelled() {
                tracing::debug!("Skipped cancelled uv metadata request");
                None
            } else {
                let target = job.task.metadata_target();
                match &target {
                    MetadataTarget::Workspace(path) => {
                        tracing::info!("Reading workspace metadata for `{path}`");
                    }
                    MetadataTarget::Script { path, .. } => {
                        tracing::info!("Synchronizing script `{path}`");
                    }
                }
                if let Some(progress) = job.progress.as_mut() {
                    progress.started();
                }

                let output = self.uv.execute(&*self.executor, &target);

                if let Some(progress) = job.progress.as_mut() {
                    progress.finished();
                }

                Some(output)
            };

            // Send the result
            // The receiver disappears when the owning project is dropped.
            if job
                .result_sender
                .send(UvMetadataResult {
                    task: job.task,
                    output,
                    progress: job.progress,
                })
                .is_ok()
            {
                // Signal that there's a new result.
                let _ = job.wake_sender.try_send(());
            }
        }
    }
}

struct UvJob {
    /// The request to return with the synchronization result.
    task: UvSyncTask,

    /// Checked before invoking uv; does not interrupt an already running process.
    cancellation: CancellationToken,

    /// The sender end of the channel communicating with the sync service.
    result_sender: Sender<UvMetadataResult>,

    /// The wake signal that notifies that there's a new result to process.
    wake_sender: Sender<()>,
    progress: Option<Box<dyn UvSyncProgress>>,
    span: tracing::Span,
}

fn worker_disconnected() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "uv synchronization worker terminated unexpectedly",
    )
}
