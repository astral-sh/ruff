use crate::cancellation::CancellationToken;
use crate::db::SourceDb;
use crate::files::FileId;
use crate::lint::Diagnostics;
use crate::program::Program;
use rayon::max_num_threads;
use rustc_hash::FxHashSet;
use std::num::NonZeroUsize;

impl Program {
    /// Checks all open files in the workspace and its dependencies.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(
        &self,
        scheduler: &dyn CheckScheduler,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<String>, CheckError> {
        let check_loop = CheckFilesLoop::new(scheduler, cancellation_token);

        check_loop.run(self.workspace().open_files.iter().copied())
    }

    /// Checks a single file and its dependencies.
    #[tracing::instrument(level = "debug", skip(self, scheduler, cancellation_token))]
    pub fn check_file(
        &self,
        file: FileId,
        scheduler: &dyn CheckScheduler,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<String>, CheckError> {
        let check_loop = CheckFilesLoop::new(scheduler, cancellation_token);

        check_loop.run([file].into_iter())
    }

    #[tracing::instrument(level = "debug", skip(self, context))]
    fn check_file_with_context(
        &self,
        file: FileId,
        context: &CheckContext,
    ) -> Result<Diagnostics, CheckError> {
        context.cancelled_ok()?;

        // TODO schedule the dependencies.
        let mut diagnostics = Vec::new();

        if self.workspace().is_file_open(file) {
            diagnostics.extend_from_slice(&self.lint_syntax(file));
        }

        Ok(Diagnostics::from(diagnostics))
    }
}

/// Schedules checks for files.
pub trait CheckScheduler {
    /// Schedules a check for a file.
    ///
    /// The check can either be run immediately on the current thread or the check can be queued
    /// in a thread pool and ran asynchronously.
    ///
    /// The order in which scheduled checks are executed is not guaranteed.
    ///
    /// The implementation should call [`CheckFileTask::run`] to execute the check.
    fn check_file(&self, file_task: CheckFileTask);

    /// The maximum number of checks that can be run concurrently.
    ///
    /// Returns `None` if the checks run on the current thread (no concurrency).
    fn max_concurrency(&self) -> Option<NonZeroUsize>;
}

/// Scheduler that runs checks on a rayon thread pool.
pub struct RayonCheckScheduler<'program, 'scope_ref, 'scope> {
    pub program: &'program Program,
    pub scope: &'scope_ref rayon::Scope<'scope>,
}

impl<'program, 'scope_ref, 'scope> RayonCheckScheduler<'program, 'scope_ref, 'scope> {
    pub fn new(program: &'program Program, scope: &'scope_ref rayon::Scope<'scope>) -> Self {
        Self { scope, program }
    }
}

impl<'program, 'scope_ref, 'scope> CheckScheduler
    for RayonCheckScheduler<'program, 'scope_ref, 'scope>
where
    'program: 'scope,
{
    fn check_file(&self, check_file_task: CheckFileTask) {
        let child_span =
            tracing::trace_span!("check_file", file_id = check_file_task.file_id.as_u32());
        let program = self.program;

        self.scope
            .spawn(move |_| child_span.in_scope(|| check_file_task.run(program)));
    }

    fn max_concurrency(&self) -> Option<NonZeroUsize> {
        Some(NonZeroUsize::new(max_num_threads()).unwrap_or(NonZeroUsize::MIN))
    }
}

/// Scheduler that runs all checks on the current thread.
///
/// Checks that are queued will run immediately on the current thread.
/// That means that later scheduled checks block the completion of earlier scheduled checks.
pub struct SameThreadCheckScheduler<'a> {
    program: &'a Program,
}

impl<'a> SameThreadCheckScheduler<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self { program }
    }
}

impl CheckScheduler for SameThreadCheckScheduler<'_> {
    fn check_file(&self, task: CheckFileTask) {
        task.run(self.program)
    }

    fn max_concurrency(&self) -> Option<NonZeroUsize> {
        None
    }
}

#[derive(Debug, Clone)]
pub enum CheckError {
    Cancelled,
}

#[derive(Debug)]
pub struct CheckFileTask {
    file_id: FileId,
    context: CheckContext,
}

impl CheckFileTask {
    /// Runs the check and communicates the result to the orchestrator.
    pub fn run(self, program: &Program) {
        match program.check_file_with_context(self.file_id, &self.context) {
            Ok(diagnostics) => self
                .context
                .sender
                .send(CheckFileMessage::Completed(diagnostics))
                .unwrap(),
            Err(CheckError::Cancelled) => self
                .context
                .sender
                .send(CheckFileMessage::Cancelled)
                .unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
struct CheckContext {
    cancellation_token: CancellationToken,
    sender: crossbeam_channel::Sender<CheckFileMessage>,
}

impl CheckContext {
    fn new(
        cancellation_token: CancellationToken,
        sender: crossbeam_channel::Sender<CheckFileMessage>,
    ) -> Self {
        Self {
            sender,
            cancellation_token,
        }
    }

    /// Queues a new file for checking using the [`CheckScheduler`].
    #[allow(unused)]
    fn schedule_check_file(&self, file_id: FileId) {
        self.sender.send(CheckFileMessage::Queue(file_id)).unwrap();
    }

    /// Returns `true` if the check has been cancelled.
    fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    fn cancelled_ok(&self) -> Result<(), CheckError> {
        if self.is_cancelled() {
            Err(CheckError::Cancelled)
        } else {
            Ok(())
        }
    }
}

struct CheckFilesLoop<'a> {
    scheduler: &'a dyn CheckScheduler,
    cancellation_token: CancellationToken,
    pending: usize,
    queued_files: FxHashSet<FileId>,
}

impl<'a> CheckFilesLoop<'a> {
    fn new(scheduler: &'a dyn CheckScheduler, cancellation_token: CancellationToken) -> Self {
        Self {
            scheduler,
            cancellation_token,

            queued_files: FxHashSet::default(),
            pending: 0,
        }
    }

    fn run(mut self, files: impl Iterator<Item = FileId>) -> Result<Vec<String>, CheckError> {
        let (sender, receiver) = if let Some(max_concurrency) = self.scheduler.max_concurrency() {
            crossbeam_channel::bounded(max_concurrency.get())
        } else {
            // The checks run on the current thread. That means it is necessary to store all messages
            // or we risk deadlocking when the main loop never gets a chance to read the messages.
            crossbeam_channel::unbounded()
        };

        let context = CheckContext::new(self.cancellation_token.clone(), sender.clone());

        for file in files {
            self.queue_file(file, context.clone())?;
        }

        self.run_impl(receiver, context)
    }

    fn run_impl(
        mut self,
        receiver: crossbeam_channel::Receiver<CheckFileMessage>,
        context: CheckContext,
    ) -> Result<Vec<String>, CheckError> {
        if self.cancellation_token.is_cancelled() {
            return Err(CheckError::Cancelled);
        }

        let mut result = Vec::default();

        for message in receiver {
            match message {
                CheckFileMessage::Completed(diagnostics) => {
                    result.extend_from_slice(&diagnostics);

                    self.pending -= 1;

                    if self.pending == 0 {
                        break;
                    }
                }
                CheckFileMessage::Queue(id) => {
                    self.queue_file(id, context.clone())?;
                }
                CheckFileMessage::Cancelled => {
                    return Err(CheckError::Cancelled);
                }
            }
        }

        Ok(result)
    }

    fn queue_file(&mut self, file_id: FileId, context: CheckContext) -> Result<(), CheckError> {
        if context.is_cancelled() {
            return Err(CheckError::Cancelled);
        }

        if self.queued_files.insert(file_id) {
            self.pending += 1;

            self.scheduler
                .check_file(CheckFileTask { file_id, context });
        }

        Ok(())
    }
}

enum CheckFileMessage {
    Completed(Diagnostics),
    Queue(FileId),
    Cancelled,
}
