use std::process::Output;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crossbeam::channel::{Receiver, SendTimeoutError, Sender, TryRecvError};
use ruff_db::files::File;
use ruff_db::system::{CommandExecutor, System, SystemPathBuf, WhichError};

use super::{MetadataTarget, Uv, unsupported_command_execution, uv_executable_error};
use crate::{Db, ProjectDatabase, ScriptSyncProgress};

const MAX_UV_WORKERS: usize = 2;
const MAX_QUEUED_UV_TASKS: usize = 8;
const CANCELLATION_CHECK_INTERVAL: Duration = Duration::from_millis(1);

/// Identifies the script metadata and Python override used to build an environment.
pub(crate) type ScriptEnvironmentCacheKey = u64;

/// A standalone script environment that should be synchronized by uv.
#[derive(Debug)]
pub struct ScriptSyncTask {
    pub(in crate::metadata) file: File,
    pub(in crate::metadata) path: SystemPathBuf,
    pub(in crate::metadata) python: Option<SystemPathBuf>,
    pub(in crate::metadata) cache_key: ScriptEnvironmentCacheKey,
}

impl ScriptSyncTask {
    /// Returns the script whose environment should be synchronized.
    pub fn file(&self) -> File {
        self.file
    }
}

/// The result of synchronizing a standalone script environment.
///
/// This owns the progress guard so progress remains active until the result is consumed.
pub struct ScriptSyncResult {
    pub(in crate::metadata) task: ScriptSyncTask,
    pub(in crate::metadata) output: std::io::Result<Output>,
    pub(in crate::metadata) progress: Option<Box<dyn ScriptSyncProgress>>,
}

impl std::fmt::Debug for ScriptSyncResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptSyncResult")
            .field("task", &self.task)
            .field("output", &self.output)
            .finish_non_exhaustive()
    }
}

impl ScriptSyncResult {
    /// Returns the absolute path of the synchronized script.
    pub fn path(&self) -> &ruff_db::system::SystemPath {
        &self.task.path
    }
}

/// A cloneable handle to one lazily started pool of uv workers.
#[derive(Clone, Default)]
pub(crate) struct UvExecutor(Arc<OnceLock<std::io::Result<UvWorkerPool>>>);

impl UvExecutor {
    pub(in crate::metadata) fn run(
        &self,
        db: &dyn Db,
        task: ScriptSyncTask,
    ) -> std::io::Result<Output> {
        let workers = self.worker_pool(db.system())?;
        let (_cancellation_sender, cancellation_receiver) = crossbeam::channel::bounded::<()>(0);
        let (result_sender, result_receiver) = crossbeam::channel::bounded(1);
        let mut request = UvJob {
            task,
            progress: None,
            result: result_sender,
            cancellation: Some(cancellation_receiver),
            span: tracing::Span::current(),
        };

        loop {
            match workers
                .requests
                .send_timeout(request, CANCELLATION_CHECK_INTERVAL)
            {
                Ok(()) => break,
                Err(SendTimeoutError::Timeout(pending)) => {
                    db.unwind_if_revision_cancelled();
                    request = pending;
                }
                Err(SendTimeoutError::Disconnected(_)) => return Err(worker_disconnected()),
            }
        }

        Self::wait_for(db, &result_receiver)
    }

    fn worker_pool(&self, system: &dyn System) -> std::io::Result<&UvWorkerPool> {
        match self.0.get_or_init(|| {
            let command_executor = system
                .command_executor()
                .ok_or_else(unsupported_command_execution)?;
            let uv = Uv::new(system);
            UvWorkerPool::new(command_executor, &uv)
        }) {
            Ok(workers) => Ok(workers),
            Err(error) => Err(std::io::Error::new(error.kind(), error.to_string())),
        }
    }

    fn wait_for(db: &dyn Db, result: &Receiver<ScriptSyncResult>) -> std::io::Result<Output> {
        loop {
            match result.recv_timeout(CANCELLATION_CHECK_INTERVAL) {
                Ok(result) => return result.output,
                Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                    db.unwind_if_revision_cancelled();
                }
                Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                    return Err(worker_disconnected());
                }
            }
        }
    }
}

impl std::panic::RefUnwindSafe for UvExecutor {}

/// Runs asynchronously requested script synchronizations with bounded request and result queues.
///
/// Scheduling applies backpressure while selecting over both sides of the pool. Receiving a result
/// can unblock a worker waiting to publish, while sending the pending job can make progress when a
/// blocking job frees request capacity without publishing to this service's result queue.
pub struct UvSyncService {
    executor: UvExecutor,
    results_sender: Sender<ScriptSyncResult>,
    results: Receiver<ScriptSyncResult>,
}

impl Default for UvSyncService {
    fn default() -> Self {
        Self::from_executor(UvExecutor::default())
    }
}

impl UvSyncService {
    /// Creates a service that shares the project's worker pool.
    pub fn from_project(db: &ProjectDatabase) -> Self {
        Self::from_executor(db.script_environments().executor().clone())
    }

    fn from_executor(executor: UvExecutor) -> Self {
        let (results_sender, results) = crossbeam::channel::bounded(MAX_QUEUED_UV_TASKS);
        Self {
            executor,
            results_sender,
            results,
        }
    }

    /// Returns a receiver for completed synchronizations.
    ///
    /// The receiver can be cloned, but callers should designate one receiver as the consumer: a
    /// cloned crossbeam receiver distributes results rather than broadcasting them.
    pub fn results(&self) -> Receiver<ScriptSyncResult> {
        self.results.clone()
    }

    /// Admits a script synchronization while making progress on completed work.
    pub fn schedule(
        &self,
        system: &dyn System,
        task: ScriptSyncTask,
        progress: Option<Box<dyn ScriptSyncProgress>>,
    ) -> Vec<ScriptSyncResult> {
        let workers = match self.executor.worker_pool(system) {
            Ok(workers) => workers,
            Err(error) => {
                return vec![ScriptSyncResult {
                    task,
                    output: Err(error),
                    progress,
                }];
            }
        };

        let path = task.path.clone();
        let request = UvJob {
            task,
            progress,
            result: self.results_sender.clone(),
            cancellation: None,
            span: tracing::debug_span!(
                "sync_script_environment",
                script = %path,
            ),
        };
        let mut completed = Vec::new();

        loop {
            crossbeam::channel::select_biased! {
                recv(self.results) -> result => {
                    match result {
                        Ok(result) => completed.push(result),
                        Err(_) => {
                            completed.push(request.into_result(Err(worker_disconnected())));
                            return completed;
                        }
                    }
                }
                send(workers.requests, request) -> result => {
                    match result {
                        Ok(()) => {
                            tracing::debug!("Queued script synchronization for `{path}`");
                        }
                        Err(error) => {
                            completed.push(
                                error.into_inner().into_result(Err(worker_disconnected())),
                            );
                        }
                    }
                    return completed;
                }
            }
        }
    }
}

/// A bounded pool for executing uv synchronization without retaining a database.
struct UvWorkerPool {
    requests: Sender<UvJob>,
}

impl UvWorkerPool {
    /// Creates worker threads using a detached command executor and resolved uv executable.
    fn new(
        command_executor: &dyn CommandExecutor,
        uv: &Result<Uv, WhichError>,
    ) -> std::io::Result<Self> {
        let (requests, receiver) = crossbeam::channel::bounded(MAX_QUEUED_UV_TASKS);
        let workers = ruff_db::max_parallelism().get().min(MAX_UV_WORKERS);
        for index in 0..workers {
            let worker = UvWorker {
                executor: command_executor.dyn_clone(),
                uv: uv.clone(),
                requests: receiver.clone(),
            };

            let _ = std::thread::Builder::new()
                .name(format!("ty-uv-sync-{index}"))
                .spawn(move || worker.run())?;
        }

        tracing::debug!(
            "Started {workers} uv synchronization workers with a queue capacity of {}",
            MAX_QUEUED_UV_TASKS
        );

        Ok(Self { requests })
    }
}

struct UvJob {
    task: ScriptSyncTask,
    progress: Option<Box<dyn ScriptSyncProgress>>,
    result: Sender<ScriptSyncResult>,
    cancellation: Option<Receiver<()>>,
    span: tracing::Span,
}

impl UvJob {
    fn complete(self, output: std::io::Result<Output>) {
        let Self {
            task,
            progress,
            result,
            ..
        } = self;

        let _ = result.send(ScriptSyncResult {
            task,
            output,
            progress,
        });
    }

    fn into_result(self, output: std::io::Result<Output>) -> ScriptSyncResult {
        ScriptSyncResult {
            task: self.task,
            output,
            progress: self.progress,
        }
    }
}

struct UvWorker {
    executor: Box<dyn CommandExecutor>,
    uv: Result<Uv, WhichError>,
    requests: Receiver<UvJob>,
}

impl UvWorker {
    fn run(self) {
        for job in &self.requests {
            if let Some(cancellation) = &job.cancellation
                && matches!(cancellation.try_recv(), Err(TryRecvError::Disconnected))
            {
                tracing::debug!(
                    "Discarded cancelled script synchronization for `{}`",
                    job.task.path
                );
                continue;
            }

            let output = self.execute(&job);
            job.complete(output);
        }
    }

    fn execute(&self, request: &UvJob) -> std::io::Result<Output> {
        let _span = request.span.enter();
        tracing::info!("Synchronizing script `{}`", request.task.path);

        let uv = self
            .uv
            .as_ref()
            .map_err(|error| uv_executable_error(*error))?;
        let target = MetadataTarget::Script {
            path: &request.task.path,
            python: request.task.python.as_deref(),
        };

        let output = uv.execute(self.executor.as_ref(), target);

        if output.as_ref().is_ok_and(|output| output.status.success()) {
            tracing::debug!("Successfully synchronized script `{}`", request.task.path);
        } else {
            tracing::debug!("Failed to synchronize script `{}`", request.task.path);
        }

        output
    }
}

fn worker_disconnected() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "uv synchronization worker terminated unexpectedly",
    )
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;
    use std::time::Duration;

    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::{
        DbWithWritableSystem, OsSystem, System as _, SystemPath, SystemPathBuf, TestSystem,
    };
    use salsa::{Cancelled, Database as _};
    use ty_static::EnvVars;

    use super::{ScriptSyncResult, ScriptSyncTask, UvExecutor, UvJob, UvSyncService, UvWorkerPool};
    use crate::db::testing::TestDb;
    use crate::{Db as _, ProgressReporter, ProjectDatabase, ProjectMetadata, ScriptSyncProgress};

    struct NoopScriptSyncProgress;

    impl ScriptSyncProgress for NoopScriptSyncProgress {}

    struct PanickingScriptSyncProgress;

    impl ScriptSyncProgress for PanickingScriptSyncProgress {}

    impl Drop for PanickingScriptSyncProgress {
        fn drop(&mut self) {
            panic!("progress failed");
        }
    }

    struct PanickingProgressReporter;

    impl ProgressReporter for PanickingProgressReporter {
        fn set_files(&mut self, _files: usize) {}

        fn for_script(
            &self,
            _db: &dyn crate::Db,
            _file: File,
        ) -> Option<Box<dyn ScriptSyncProgress>> {
            Some(Box::new(PanickingScriptSyncProgress))
        }

        fn report_checked_file(
            &self,
            _db: &ProjectDatabase,
            _file: File,
            _diagnostics: &[ruff_db::diagnostic::Diagnostic],
        ) {
        }

        fn report_diagnostics(
            &mut self,
            _db: &ProjectDatabase,
            _diagnostics: Vec<ruff_db::diagnostic::Diagnostic>,
        ) {
        }
    }

    #[test]
    fn uv_resolution_is_lazy_and_cached() -> anyhow::Result<()> {
        let current_exe = SystemPathBuf::from_path_buf(std::env::current_exe()?)
            .map_err(|path| anyhow::anyhow!("non-UTF-8 test executable: {}", path.display()))?;
        let cwd = current_exe
            .parent()
            .ok_or_else(|| anyhow::anyhow!("test executable has no parent"))?;
        let missing = cwd.join("__ty_missing_uv_executable__");

        let system = TestSystem::new(OsSystem::new(cwd));
        assert!(!system.path_exists(&missing));
        system.set_env_var(EnvVars::UV, missing.as_str());
        let executor = UvExecutor::default();
        assert!(executor.0.get().is_none());

        system.set_env_var(EnvVars::UV, current_exe.as_str());
        let first = executor.worker_pool(&system)?;

        system.set_env_var(EnvVars::UV, missing.as_str());
        let second = executor.worker_pool(&system)?;
        assert!(std::ptr::eq(first, second));

        Ok(())
    }

    #[test]
    fn progress_panic_propagates_to_blocking_caller() -> anyhow::Result<()> {
        let root = SystemPath::new("/project").to_path_buf();
        let path = root.join("script.py");
        let mut db = TestDb::new(ProjectMetadata::new("test", root));
        db.writable_system().set_env_var(EnvVars::TY_UV, "scripts");
        db.write_file(&path, "# /// script\n# dependencies = []\n# ///\n")?;
        let file = system_path_to_file(&db, &path)?;
        let reporter = PanickingProgressReporter;

        let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
            db.script_environments()
                .ensure_environment_initialized(&db, file, Some(&reporter));
        }));

        assert_eq!(
            panic
                .expect_err("finishing progress should panic on the checking thread")
                .downcast_ref::<&str>(),
            Some(&"progress failed")
        );

        Ok(())
    }

    #[test]
    fn database_write_cancels_pending_uv_initialization() -> anyhow::Result<()> {
        let root = SystemPath::new("/project").to_path_buf();
        let path = root.join("script.py");
        let mut db = TestDb::new(ProjectMetadata::new("test", root));
        db.writable_system().set_env_var(EnvVars::TY_UV, "scripts");
        db.write_file(&path, "# /// script\n# dependencies = []\n# ///\n")?;
        let file = system_path_to_file(&db, &path)?;
        let snapshot = db.clone();
        let environments = db.script_environments().clone();
        let checking_environments = environments.clone();

        let (request_sender, request_receiver) = crossbeam::channel::bounded(1);
        assert!(
            db.script_environments()
                .executor()
                .0
                .set(Ok(UvWorkerPool {
                    requests: request_sender,
                }))
                .is_ok()
        );

        let checking = std::thread::spawn(move || {
            Cancelled::catch(AssertUnwindSafe(|| {
                checking_environments.ensure_environment_initialized(&snapshot, file, None);
            }))
        });

        let _request = request_receiver.recv_timeout(Duration::from_secs(5))?;

        // Scheduling an asynchronous refresh must not wait for the checking thread's uv call.
        // It briefly observes the running synchronization and returns.
        let preparing_environments = environments.clone();
        let preparing_snapshot = db.clone();
        let (prepared_sender, prepared_receiver) = crossbeam::channel::bounded(1);
        let preparing = std::thread::spawn(move || {
            let result = Cancelled::catch(AssertUnwindSafe(|| {
                preparing_environments
                    .prepare_sync(&preparing_snapshot, file)
                    .is_some()
            }));
            if matches!(result, Ok(false)) {
                let _ = prepared_sender.send(());
            }
            result
        });
        let prepared_without_waiting = prepared_receiver
            .recv_timeout(Duration::from_secs(5))
            .is_ok();

        db.trigger_cancellation();

        let result = checking
            .join()
            .map_err(|_| anyhow::anyhow!("checking thread panicked"))?;
        assert!(matches!(result, Err(Cancelled::PendingWrite)));
        let prepare_result = preparing
            .join()
            .map_err(|_| anyhow::anyhow!("preparing thread panicked"))?;
        assert!(prepared_without_waiting);
        assert!(matches!(prepare_result, Ok(false)));
        assert!(environments.prepare_sync(&db, file).is_some());

        Ok(())
    }

    #[test]
    fn scheduling_makes_progress_when_shared_queue_contains_blocking_jobs() -> anyhow::Result<()> {
        let root = SystemPath::new("/project").to_path_buf();
        let first_path = root.join("first.py");
        let mut db = TestDb::new(ProjectMetadata::new("test", root));
        db.write_file(&first_path, "# /// script\n# dependencies = []\n# ///\n")?;
        let first_file = system_path_to_file(&db, &first_path)?;

        let (request_sender, request_receiver) = crossbeam::channel::bounded(1);
        let system = TestSystem::default();
        let executor = UvExecutor::default();
        assert!(
            executor
                .0
                .set(Ok(UvWorkerPool {
                    requests: request_sender.clone(),
                }))
                .is_ok()
        );
        let service = UvSyncService::from_executor(executor);

        let task = || ScriptSyncTask {
            file: first_file,
            path: first_path.clone(),
            python: None,
            cache_key: 2,
        };

        let (blocking_result, _blocking_receiver) = crossbeam::channel::bounded(1);
        let (_cancellation_sender, cancellation) = crossbeam::channel::bounded(0);
        request_sender
            .send(UvJob {
                task: task(),
                progress: None,
                result: blocking_result,
                cancellation: Some(cancellation),
                span: tracing::Span::none(),
            })
            .map_err(|_| anyhow::anyhow!("test worker request queue disconnected"))?;

        service
            .results_sender
            .send(ScriptSyncResult {
                task: task(),
                output: Err(std::io::Error::other("test result")),
                progress: None,
            })
            .map_err(|_| anyhow::anyhow!("script result receiver disconnected"))?;
        let pending = task();
        let scheduling = std::thread::spawn(move || {
            service.schedule(&system, pending, Some(Box::new(NoopScriptSyncProgress)))
        });

        let _first_request = request_receiver.recv_timeout(Duration::from_secs(1))?;
        let completed = scheduling
            .join()
            .map_err(|_| anyhow::anyhow!("scheduling thread panicked"))?;
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].task.file, first_file);

        let second_request = request_receiver.recv_timeout(Duration::from_secs(1))?;
        assert_eq!(second_request.task.path, first_path);
        assert!(second_request.progress.is_some());

        Ok(())
    }
}
