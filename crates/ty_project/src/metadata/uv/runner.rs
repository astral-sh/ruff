use std::process::Output;
use std::sync::{Arc, OnceLock};

use crossbeam::channel::{Receiver, Sender};
use ruff_db::system::{CommandExecutor, System, SystemPathBuf, WhichError};

use super::{MetadataTarget, Uv, unsupported_command_execution, uv_executable_error};

const MAX_UV_WORKERS: usize = 2;
const MAX_QUEUED_UV_TASKS: usize = 8;

/// A standalone script environment that should be synchronized by uv.
#[derive(Debug)]
pub(in crate::metadata) struct ScriptSyncTask {
    pub(in crate::metadata) path: SystemPathBuf,
    pub(in crate::metadata) python: Option<SystemPathBuf>,
}

/// A cloneable handle to one lazily started pool of uv workers.
#[derive(Clone, Default)]
pub(in crate::metadata) struct UvExecutor(Arc<OnceLock<std::io::Result<UvWorkerPool>>>);

impl UvExecutor {
    pub(in crate::metadata) fn run(
        &self,
        system: &dyn System,
        task: ScriptSyncTask,
    ) -> std::io::Result<Output> {
        let workers = self.worker_pool(system)?;
        let (result_sender, result_receiver) = crossbeam::channel::bounded(1);

        workers
            .requests
            .send(UvJob {
                task,
                result: result_sender,
                span: tracing::Span::current(),
            })
            .map_err(|_| worker_disconnected())?;

        result_receiver
            .recv()
            .unwrap_or_else(|_| Err(worker_disconnected()))
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
}

impl std::panic::RefUnwindSafe for UvExecutor {}

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
    result: Sender<std::io::Result<Output>>,
    span: tracing::Span,
}

struct UvWorker {
    executor: Box<dyn CommandExecutor>,
    uv: Result<Uv, WhichError>,
    requests: Receiver<UvJob>,
}

impl UvWorker {
    fn run(self) {
        for job in &self.requests {
            let output = self.execute(&job);
            let _ = job.result.send(output);
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
    use ruff_db::system::{OsSystem, System as _, SystemPathBuf, TestSystem};
    use ty_static::EnvVars;

    use super::UvExecutor;

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
}
