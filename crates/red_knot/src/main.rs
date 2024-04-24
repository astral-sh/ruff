use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tracing::subscriber::Interest;
use tracing::{Level, Metadata};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::{Layer, Registry};
use tracing_tree::time::Uptime;

// use red_knot::watch::FileWatcher;
use red_knot::cancellation::CancellationSource;
use red_knot::db::{HasJar, SourceDb, SourceJar};
use red_knot::files::FileId;
use red_knot::module::{ModuleSearchPath, ModuleSearchPathKind};
use red_knot::program::{FileChange, FileChangeKind, Program};
use red_knot::watch::FileWatcher;
use red_knot::{files, Workspace};

#[allow(
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::unnecessary_wraps,
    clippy::print_stderr
)]
fn main() -> anyhow::Result<()> {
    setup_tracing();

    let arguments: Vec<_> = std::env::args().collect();

    if arguments.len() < 2 {
        eprintln!("Usage: red_knot <path>");
        return Err(anyhow::anyhow!("Invalid arguments"));
    }

    let entry_point = Path::new(&arguments[1]);

    if !entry_point.exists() {
        eprintln!("The entry point does not exist.");
        return Err(anyhow::anyhow!("Invalid arguments"));
    }

    if !entry_point.is_file() {
        eprintln!("The entry point is not a file.");
        return Err(anyhow::anyhow!("Invalid arguments"));
    }

    let files = files::Files::default();
    let workspace_folder = entry_point.parent().unwrap();
    let mut workspace = Workspace::new(workspace_folder.to_path_buf());

    let workspace_search_path = ModuleSearchPath::new(
        workspace.root().to_path_buf(),
        ModuleSearchPathKind::FirstParty,
    );

    let entry_id = files.intern(entry_point);

    let mut program = Program::new(vec![workspace_search_path], files.clone());

    workspace.open_file(entry_id);

    let (sender, receiver) = crossbeam_channel::bounded(
        std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(50)
            .max(4), // TODO: Both these numbers are very arbitrary. Pick sensible defaults.
    );

    // Listen to Ctrl+C and abort the watch mode.
    let abort_sender = Mutex::new(Some(sender.clone()));
    ctrlc::set_handler(move || {
        let mut lock = abort_sender.lock().unwrap();

        if let Some(sender) = lock.take() {
            sender.send(Message::Exit).unwrap();
        }
    })?;

    // Watch for file changes and re-trigger the analysis.
    let file_changes_sender = sender.clone();

    let mut file_watcher = FileWatcher::new(
        move |changes| {
            file_changes_sender
                .send(Message::FileChanges(changes))
                .unwrap();
        },
        files.clone(),
    );

    file_watcher.watch_folder(workspace_folder)?;

    let files_to_check = vec![entry_id];

    // Main loop that runs until the user exits the program
    // Runs the analysis for each changed file. Cancels the analysis if a new change is detected.
    loop {
        let changes = {
            tracing::trace!("Main Loop: Tick");

            // Token to cancel the analysis if a new change is detected.
            let run_cancellation_token_source = CancellationSource::new();
            let run_cancellation_token = run_cancellation_token_source.token();

            // Tracks the number of pending analysis runs.
            let pending_analysis = Arc::new(AtomicUsize::new(0));

            // Take read-only references that are copy and Send.
            let program = &program;
            let workspace = &workspace;

            let receiver = receiver.clone();
            let started_analysis = pending_analysis.clone();

            // Orchestration task. Ideally, we would run this on main but we should start it as soon as possible so that
            // we avoid scheduling tasks when we already know that we're about to exit or cancel the analysis because of a file change.
            // This uses `std::thread::spawn` because we don't want it to run inside of the thread pool
            // or this code deadlocks when using a thread pool of the size 1.
            let orchestration_handle = std::thread::spawn(move || {
                fn consume_pending_messages(
                    receiver: &crossbeam_channel::Receiver<Message>,
                    mut aggregated_changes: AggregatedChanges,
                ) -> NextTickCommand {
                    loop {
                        // Consume possibly incoming file change messages before running a new analysis, but don't wait for more than 100ms.
                        crossbeam_channel::select! {
                            recv(receiver) -> message => {
                                match message {
                                    Ok(Message::Exit) => {
                                        return NextTickCommand::Exit;
                                    }
                                    Ok(Message::FileChanges(file_changes)) => {
                                        aggregated_changes.extend(file_changes);
                                    }

                                    Ok(Message::AnalysisCancelled | Message::AnalysisCompleted(_)) => {
                                        unreachable!(
                                            "All analysis should have been completed at this time"
                                        );
                                    },

                                    Err(_) => {
                                        // There are no more senders, no point in waiting for more messages
                                        break;
                                    }
                                }
                            },
                            default(std::time::Duration::from_millis(100)) => {
                                break;
                            }
                        }
                    }

                    NextTickCommand::FileChanges(aggregated_changes)
                }

                let mut diagnostics = Vec::new();
                let mut aggregated_changes = AggregatedChanges::default();

                for message in &receiver {
                    match message {
                        Message::AnalysisCompleted(file_diagnostics) => {
                            diagnostics.extend_from_slice(&file_diagnostics);

                            if pending_analysis.fetch_sub(1, Ordering::SeqCst) == 1 {
                                // Analysis completed, print the diagnostics.
                                dbg!(&diagnostics);
                            }
                        }

                        Message::AnalysisCancelled => {
                            if pending_analysis.fetch_sub(1, Ordering::SeqCst) == 1 {
                                return consume_pending_messages(&receiver, aggregated_changes);
                            }
                        }

                        Message::Exit => {
                            run_cancellation_token_source.cancel();

                            // Don't consume any outstanding messages because we're exiting anyway.
                            return NextTickCommand::Exit;
                        }

                        Message::FileChanges(changes) => {
                            // Request cancellation, but wait until all analysis tasks have completed to
                            // avoid stale messages in the next main loop.
                            run_cancellation_token_source.cancel();

                            aggregated_changes.extend(changes);

                            if pending_analysis.load(Ordering::SeqCst) == 0 {
                                return consume_pending_messages(&receiver, aggregated_changes);
                            }
                        }
                    }
                }

                // This can be reached if there's no Ctrl+C and no file watcher handler.
                // In that case, assume that we don't run in watch mode and exit.
                NextTickCommand::Exit
            });

            // Star the analysis task on the thread pool and wait until they complete.
            rayon::scope(|scope| {
                for file in &files_to_check {
                    let cancellation_token = run_cancellation_token.clone();
                    if cancellation_token.is_cancelled() {
                        break;
                    }

                    let sender = sender.clone();

                    started_analysis.fetch_add(1, Ordering::SeqCst);

                    // TODO: How do we allow the host to control the number of threads used?
                    //  Or should we just assume that each host implements its own main loop,
                    //  I don't think that's entirely unreasonable but we should avoid
                    //  having different main loops per host AND command (e.g. format vs check vs lint)
                    scope.spawn(move |_| {
                        if cancellation_token.is_cancelled() {
                            tracing::trace!("Exit analysis because cancellation was requested.");
                            sender.send(Message::AnalysisCancelled).unwrap();
                            return;
                        }

                        // TODO schedule the dependencies.
                        let mut diagnostics = Vec::new();

                        if workspace.is_file_open(*file) {
                            diagnostics.extend_from_slice(&program.lint_syntax(*file));
                        }

                        sender
                            .send(Message::AnalysisCompleted(diagnostics))
                            .unwrap();
                    });
                }
            });

            // Wait for the orchestration task to complete. This either returns the file changes
            // or instructs the main loop to exit.
            match orchestration_handle.join().unwrap() {
                NextTickCommand::FileChanges(changes) => changes,
                NextTickCommand::Exit => {
                    break;
                }
            }
        };

        // We have a mutable reference here and can perform all necessary invalidations.
        program.apply_changes(changes.iter());
    }

    let source_jar: &SourceJar = program.jar();

    dbg!(source_jar.parsed.statistics());
    dbg!(source_jar.sources.statistics());

    Ok(())
}

enum Message {
    AnalysisCompleted(Vec<String>),
    AnalysisCancelled,
    Exit,
    FileChanges(Vec<FileChange>),
}

#[derive(Default, Debug)]
struct AggregatedChanges {
    changes: FxHashMap<FileId, FileChangeKind>,
}

impl AggregatedChanges {
    fn add(&mut self, change: FileChange) {
        match self.changes.entry(change.file_id()) {
            Entry::Occupied(mut entry) => {
                let merged = entry.get_mut();

                match (merged, change.kind()) {
                    (FileChangeKind::Created, FileChangeKind::Deleted) => {
                        // Creation after deletion means that ruff newer saw the file.
                        entry.remove();
                    }
                    (FileChangeKind::Created, FileChangeKind::Modified) => {
                        // No-op, for ruff, modifying a file that it doesn't yet know that it exists is still considered a creation.
                    }

                    (FileChangeKind::Modified, FileChangeKind::Created) => {
                        // Uhh, that should probably not happen. Continue considering it a modification.
                    }

                    (FileChangeKind::Modified, FileChangeKind::Deleted) => {
                        *entry.get_mut() = FileChangeKind::Deleted;
                    }

                    (FileChangeKind::Deleted, FileChangeKind::Created) => {
                        *entry.get_mut() = FileChangeKind::Modified;
                    }

                    (FileChangeKind::Deleted, FileChangeKind::Modified) => {
                        // That's weird, but let's consider it a modification.
                        *entry.get_mut() = FileChangeKind::Modified;
                    }

                    (FileChangeKind::Created, FileChangeKind::Created)
                    | (FileChangeKind::Modified, FileChangeKind::Modified)
                    | (FileChangeKind::Deleted, FileChangeKind::Deleted) => {
                        // No-op transitions. Some of them should be impossible but we handle them anyway.
                    }
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(change.kind());
            }
        }
    }

    fn extend<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileChange>,
        I::IntoIter: ExactSizeIterator,
    {
        let iter = changes.into_iter();
        self.changes.reserve(iter.len());

        for change in iter {
            self.add(change);
        }
    }

    fn iter(&self) -> impl Iterator<Item = FileChange> + '_ {
        self.changes
            .iter()
            .map(|(id, kind)| FileChange::new(*id, *kind))
    }
}

enum NextTickCommand {
    /// Exit the main loop in the next tick
    Exit,
    /// Apply the given changes in the next main loop tick.
    FileChanges(AggregatedChanges),
}

fn setup_tracing() {
    let subscriber = Registry::default().with(
        tracing_tree::HierarchicalLayer::default()
            .with_indent_lines(true)
            .with_indent_amount(2)
            .with_bracketed_fields(true)
            .with_targets(true)
            .with_writer(|| Box::new(std::io::stderr()))
            .with_timer(Uptime::default())
            .with_filter(LoggingFilter {
                trace_level: Level::TRACE,
            }),
    );

    tracing::subscriber::set_global_default(subscriber).unwrap();
}

struct LoggingFilter {
    trace_level: Level,
}

impl LoggingFilter {
    fn is_enabled(&self, meta: &Metadata<'_>) -> bool {
        let filter = if meta.target().starts_with("red_knot") || meta.target().starts_with("ruff") {
            self.trace_level
        } else {
            Level::INFO
        };

        meta.level() <= &filter
    }
}

impl<S> Filter<S> for LoggingFilter {
    fn enabled(&self, meta: &Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        self.is_enabled(meta)
    }

    fn callsite_enabled(&self, meta: &'static Metadata<'static>) -> Interest {
        if self.is_enabled(meta) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::from_level(self.trace_level))
    }
}
