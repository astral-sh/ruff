use rayon::{current_num_threads, yield_local};
use rustc_hash::FxHashSet;
use salsa::{DebugWithDb, ParallelDatabase};

use ruff_db::vfs::{Vfs, VfsFile};

use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
use crate::program::Program;

impl Program {
    /// Checks all open files in the workspace and its dependencies.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(&self, mode: ExecutionMode) -> Vec<String> {
        let mut context = CheckContext::new(self);

        match mode {
            ExecutionMode::SingleThreaded => SingleThreadedExecutor.run(&mut context),
            ExecutionMode::ThreadPool => ThreadPoolExecutor.run(&mut context),
        };

        context.finish()
    }

    #[tracing::instrument(level = "debug", skip(self, context))]
    fn check_file(&self, file: VfsFile, context: &CheckFileContext) -> Diagnostics {
        // let index = semantic_index(self, file)?;
        // let dependencies = index.symbol_table().dependencies();
        //
        // if !dependencies.is_empty() {
        //     let module = file_to_module(self, file)?;
        //
        //     // TODO scheduling all dependencies here is wasteful if we don't infer any types on them
        //     //  but I think that's unlikely, so it is okay?
        //     //  Anyway, we need to figure out a way to retrieve the dependencies of a module
        //     //  from the persistent cache. So maybe it should be a separate query after all.
        //     for dependency in dependencies {
        //         let dependency_name = match dependency {
        //             Dependency::Module(name) => Some(name.clone()),
        //             Dependency::Relative { .. } => match &module {
        //                 Some(module) => module.resolve_dependency(self, dependency)?,
        //                 None => None,
        //             },
        //         };
        //
        //         if let Some(dependency_name) = dependency_name {
        //             // TODO We may want to have a different check functions for non-first-party
        //             //   files because we only need to index them and not check them.
        //             //   Supporting non-first-party code also requires supporting typing stubs.
        //             if let Some(dependency) = resolve_module(self, dependency_name)? {
        //                 if dependency.path(self)?.root().kind().is_first_party() {
        //                     context.schedule_dependency(dependency.path(self)?.file());
        //                 }
        //             }
        //         }
        //     }
        // }

        let mut diagnostics = Vec::new();

        if self.workspace().is_file_open(file) {
            diagnostics.extend_from_slice(lint_syntax(self, file));
            diagnostics.extend_from_slice(lint_semantic(self, file));
        }

        Diagnostics::from(diagnostics)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    SingleThreaded,
    ThreadPool,
}

/// Context that stores state information about the entire check operation.
struct CheckContext<'a> {
    /// IDs of the files that have been queued for checking.
    ///
    /// Used to avoid queuing the same file twice.
    scheduled_files: FxHashSet<VfsFile>,

    /// Reference to the program that is checked.
    program: &'a Program,

    /// The aggregated diagnostics
    diagnostics: Vec<String>,
}

impl<'a> CheckContext<'a> {
    fn new(program: &'a Program) -> Self {
        Self {
            scheduled_files: FxHashSet::default(),
            program,
            diagnostics: Vec::new(),
        }
    }

    /// Returns the tasks to check all open files in the workspace.
    fn check_open_files(&mut self) -> Vec<CheckOpenFileTask> {
        self.scheduled_files
            .extend(self.program.workspace().open_files());

        self.program
            .workspace()
            .open_files()
            .map(|file_id| CheckOpenFileTask { file_id })
            .collect()
    }

    /// Returns the task to check a dependency.
    fn check_dependency(&mut self, file_id: VfsFile) -> Option<CheckDependencyTask> {
        if self.scheduled_files.insert(file_id) {
            Some(CheckDependencyTask { file_id })
        } else {
            None
        }
    }

    /// Pushes the result for a single file check operation
    fn push_diagnostics(&mut self, diagnostics: &Diagnostics) {
        self.diagnostics.extend_from_slice(diagnostics);
    }

    /// Returns a reference to the program that is being checked.
    fn program(&self) -> &'a Program {
        self.program
    }

    /// Creates a task context that is used to check a single file.
    fn task_context<'b, S>(&self, dependency_scheduler: &'b S) -> CheckTaskContext<'b, S>
    where
        S: ScheduleDependency,
    {
        CheckTaskContext {
            program: self.program.snapshot(),
            dependency_scheduler,
        }
    }

    fn finish(self) -> Vec<String> {
        self.diagnostics
    }
}

/// Trait that abstracts away how a dependency of a file gets scheduled for checking.
trait ScheduleDependency {
    /// Schedules the file with the given ID for checking.
    fn schedule(&self, file_id: VfsFile);
}

impl<T> ScheduleDependency for T
where
    T: Fn(VfsFile),
{
    fn schedule(&self, file_id: VfsFile) {
        let f = self;
        f(file_id);
    }
}

/// Context that is used to run a single file check task.
///
/// The task is generic over `S` because it is passed across thread boundaries and
/// we don't want to add the requirement that [`ScheduleDependency`] must be [`Send`].
struct CheckTaskContext<'scheduler, S>
where
    S: ScheduleDependency,
{
    dependency_scheduler: &'scheduler S,
    program: salsa::Snapshot<Program>,
}

impl<'scheduler, S> CheckTaskContext<'scheduler, S>
where
    S: ScheduleDependency,
{
    fn as_file_context(&self) -> CheckFileContext<'scheduler> {
        CheckFileContext {
            dependency_scheduler: self.dependency_scheduler,
        }
    }
}

/// Context passed when checking a single file.
///
/// This is a trimmed down version of [`CheckTaskContext`] with the type parameter `S` erased
/// to avoid monomorphization of [`Program:check_file`].
struct CheckFileContext<'a> {
    dependency_scheduler: &'a dyn ScheduleDependency,
}

impl<'a> CheckFileContext<'a> {
    fn schedule_dependency(&self, file_id: VfsFile) {
        self.dependency_scheduler.schedule(file_id);
    }
}

#[derive(Debug)]
enum CheckFileTask {
    OpenFile(CheckOpenFileTask),
    Dependency(CheckDependencyTask),
}

impl CheckFileTask {
    /// Runs the task and returns the results for checking this file.
    fn run<S>(&self, context: &CheckTaskContext<S>) -> Diagnostics
    where
        S: ScheduleDependency,
    {
        match self {
            Self::OpenFile(task) => task.run(context),
            Self::Dependency(task) => task.run(context),
        }
    }

    fn file_id(&self) -> VfsFile {
        match self {
            CheckFileTask::OpenFile(task) => task.file_id,
            CheckFileTask::Dependency(task) => task.file_id,
        }
    }
}

/// Task to check an open file.

#[derive(Debug)]
struct CheckOpenFileTask {
    file_id: VfsFile,
}

impl CheckOpenFileTask {
    fn run<S>(&self, context: &CheckTaskContext<S>) -> Diagnostics
    where
        S: ScheduleDependency,
    {
        context
            .program
            .check_file(self.file_id, &context.as_file_context())
    }
}

/// Task to check a dependency file.
#[derive(Debug)]
struct CheckDependencyTask {
    file_id: VfsFile,
}

impl CheckDependencyTask {
    fn run<S>(&self, context: &CheckTaskContext<S>) -> Diagnostics
    where
        S: ScheduleDependency,
    {
        context
            .program
            .check_file(self.file_id, &context.as_file_context())
    }
}

/// Executor that schedules the checking of individual program files.
trait CheckExecutor {
    fn run(self, context: &mut CheckContext);
}

/// Executor that runs all check operations on the current thread.
///
/// The executor does not schedule dependencies for checking.
/// The main motivation for scheduling dependencies
/// in a multithreaded environment is to parse and index the dependencies concurrently.
/// However, that doesn't make sense in a single threaded environment, because the dependencies then compute
/// with checking the open files. Checking dependencies in a single threaded environment is more likely
/// to hurt performance because we end up analyzing files in their entirety, even if we only need to type check parts of them.
#[derive(Debug, Default)]
struct SingleThreadedExecutor;

impl CheckExecutor for SingleThreadedExecutor {
    fn run(self, context: &mut CheckContext) {
        let mut queue = context.check_open_files();

        let noop_schedule_dependency = |_| {};

        while let Some(file) = queue.pop() {
            let task_context = context.task_context(&noop_schedule_dependency);
            context.push_diagnostics(&file.run(&task_context));
        }
    }
}

/// Executor that runs the check operations on a thread pool.
///
/// The executor runs each check operation as its own task using a thread pool.
///
/// Other than [`SingleThreadedExecutor`], this executor schedules dependencies for checking. It
/// even schedules dependencies for checking when the thread pool size is 1 for a better debugging experience.
#[derive(Debug, Default)]
struct ThreadPoolExecutor;

impl CheckExecutor for ThreadPoolExecutor {
    fn run(self, context: &mut CheckContext) {
        let num_threads = current_num_threads();
        let single_threaded = num_threads == 1;
        let span = tracing::trace_span!("ThreadPoolExecutor::run", num_threads);
        let _ = span.enter();

        let mut queue: Vec<_> = context
            .check_open_files()
            .into_iter()
            .map(CheckFileTask::OpenFile)
            .collect();

        let (sender, receiver) = if single_threaded {
            // Use an unbounded queue for single threaded execution to prevent deadlocks
            // when a single file schedules multiple dependencies.
            crossbeam::channel::unbounded()
        } else {
            // Use a bounded queue to apply backpressure when the orchestration thread isn't able to keep
            // up processing messages from the worker threads.
            crossbeam::channel::bounded(num_threads)
        };

        let schedule_sender = sender.clone();
        let schedule_dependency = move |file_id| {
            schedule_sender
                .send(ThreadPoolMessage::ScheduleDependency(file_id))
                .unwrap();
        };

        rayon::in_place_scope(|scope| {
            let mut pending = 0usize;

            loop {
                // FIXME cancellation
                // context.program().cancelled()?;

                // 1. Try to get a queued message to ensure that we have always remaining space in the channel to prevent blocking the worker threads.
                // 2. Try to process a queued file
                // 3. If there's no queued file wait for the next incoming message.
                // 4. Exit if there are no more messages and no senders.
                let message = if let Ok(message) = receiver.try_recv() {
                    message
                } else if let Some(task) = queue.pop() {
                    pending += 1;

                    let task_context = context.task_context(&schedule_dependency);
                    let sender = sender.clone();
                    let task_span = tracing::trace_span!(
                        parent: &span,
                        "CheckFileTask::run",
                        file_id = format!("{:?}", task.file_id().debug(context.program)),
                    );

                    scope.spawn(move |_| {
                        task_span.in_scope(|| {
                            let result = task.run(&task_context);
                            sender.send(ThreadPoolMessage::Completed(result)).unwrap();
                        });
                    });

                    // If this is a single threaded rayon thread pool, yield the current thread
                    // or we never start processing the work items.
                    if single_threaded {
                        yield_local();
                    }

                    continue;
                } else if let Ok(message) = receiver.recv() {
                    message
                } else {
                    break;
                };

                match message {
                    ThreadPoolMessage::ScheduleDependency(dependency) => {
                        if let Some(task) = context.check_dependency(dependency) {
                            queue.push(CheckFileTask::Dependency(task));
                        }
                    }
                    ThreadPoolMessage::Completed(diagnostics) => {
                        context.push_diagnostics(&diagnostics);
                        pending -= 1;

                        if pending == 0 && queue.is_empty() {
                            break;
                        }
                    }
                }
            }
        });
    }
}

#[derive(Debug)]
enum ThreadPoolMessage {
    ScheduleDependency(VfsFile),
    Completed(Diagnostics),
}
