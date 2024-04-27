#![allow(clippy::dbg_macro)]

use std::collections::hash_map::Entry;
use std::path::Path;
use std::sync::Mutex;

use rustc_hash::FxHashMap;
use tracing::subscriber::Interest;
use tracing::{Level, Metadata};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::{Layer, Registry};
use tracing_tree::time::Uptime;

use red_knot::cancellation::CancellationTokenSource;
use red_knot::db::{HasJar, SourceDb, SourceJar};
use red_knot::files::FileId;
use red_knot::module::{ModuleSearchPath, ModuleSearchPathKind};
use red_knot::program::check::{CheckError, RayonCheckScheduler};
use red_knot::program::{FileChange, FileChangeKind, Program};
use red_knot::watch::FileWatcher;
use red_knot::Workspace;

#[allow(clippy::print_stdout, clippy::unnecessary_wraps, clippy::print_stderr)]
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

    let workspace_folder = entry_point.parent().unwrap();
    let workspace = Workspace::new(workspace_folder.to_path_buf());

    let workspace_search_path = ModuleSearchPath::new(
        workspace.root().to_path_buf(),
        ModuleSearchPathKind::FirstParty,
    );
    let mut program = Program::new(workspace, vec![workspace_search_path]);

    let entry_id = program.file_id(entry_point);
    program.workspace_mut().open_file(entry_id);

    let (main_loop, main_loop_cancellation_token) = MainLoop::new();

    // Listen to Ctrl+C and abort the watch mode.
    let main_loop_cancellation_token = Mutex::new(Some(main_loop_cancellation_token));
    ctrlc::set_handler(move || {
        let mut lock = main_loop_cancellation_token.lock().unwrap();

        if let Some(token) = lock.take() {
            token.stop();
        }
    })?;

    let file_changes_notifier = main_loop.file_changes_notifier();

    // Watch for file changes and re-trigger the analysis.
    let mut file_watcher = FileWatcher::new(
        move |changes| {
            file_changes_notifier.notify(changes);
        },
        program.files().clone(),
    )?;

    file_watcher.watch_folder(workspace_folder)?;

    main_loop.run(&mut program);

    let source_jar: &SourceJar = program.jar();

    dbg!(source_jar.parsed.statistics());
    dbg!(source_jar.sources.statistics());

    Ok(())
}

struct MainLoop {
    orchestrator_sender: crossbeam_channel::Sender<OrchestratorMessage>,
    main_loop_receiver: crossbeam_channel::Receiver<MainLoopMessage>,
}

impl MainLoop {
    fn new() -> (Self, MainLoopCancellationToken) {
        let (orchestrator_sender, orchestrator_receiver) = crossbeam_channel::bounded(1);
        let (main_loop_sender, main_loop_receiver) = crossbeam_channel::bounded(1);

        let mut orchestrator = Orchestrator {
            pending_analysis: None,
            receiver: orchestrator_receiver,
            sender: main_loop_sender.clone(),
            aggregated_changes: AggregatedChanges::default(),
        };

        std::thread::spawn(move || {
            orchestrator.run();
        });

        (
            Self {
                orchestrator_sender: orchestrator_sender.clone(),
                main_loop_receiver,
            },
            MainLoopCancellationToken {
                sender: main_loop_sender,
            },
        )
    }

    fn file_changes_notifier(&self) -> FileChangesNotifier {
        FileChangesNotifier {
            sender: self.orchestrator_sender.clone(),
        }
    }

    fn run(self, program: &mut Program) {
        self.orchestrator_sender
            .send(OrchestratorMessage::Run)
            .unwrap();

        for message in &self.main_loop_receiver {
            tracing::trace!("Main Loop: Tick");

            match message {
                MainLoopMessage::CheckProgram => {
                    // Remove mutability from program.
                    let program = &*program;
                    let run_cancellation_token_source = CancellationTokenSource::new();
                    let run_cancellation_token = run_cancellation_token_source.token();
                    let sender = &self.orchestrator_sender;

                    sender
                        .send(OrchestratorMessage::CheckProgramStarted {
                            cancellation_token: run_cancellation_token_source,
                        })
                        .unwrap();

                    rayon::in_place_scope(|scope| {
                        let scheduler = RayonCheckScheduler { program, scope };

                        let result = program.check(&scheduler, run_cancellation_token);
                        match result {
                            Ok(result) => sender
                                .send(OrchestratorMessage::CheckProgramCompleted(result))
                                .unwrap(),
                            Err(CheckError::Cancelled) => sender
                                .send(OrchestratorMessage::CheckProgramCancelled)
                                .unwrap(),
                        }
                    });
                }
                MainLoopMessage::ApplyChanges(changes) => {
                    program.apply_changes(changes.iter());
                }
                MainLoopMessage::CheckCompleted(diagnostics) => {
                    dbg!(diagnostics);
                }
                MainLoopMessage::Exit => {
                    return;
                }
            }
        }
    }
}

impl Drop for MainLoop {
    fn drop(&mut self) {
        self.orchestrator_sender
            .send(OrchestratorMessage::Shutdown)
            .unwrap();
    }
}

#[derive(Debug, Clone)]
struct FileChangesNotifier {
    sender: crossbeam_channel::Sender<OrchestratorMessage>,
}

impl FileChangesNotifier {
    fn notify(&self, changes: Vec<FileChange>) {
        self.sender
            .send(OrchestratorMessage::FileChanges(changes))
            .unwrap();
    }
}

#[derive(Debug)]
struct MainLoopCancellationToken {
    sender: crossbeam_channel::Sender<MainLoopMessage>,
}

impl MainLoopCancellationToken {
    fn stop(self) {
        self.sender.send(MainLoopMessage::Exit).unwrap();
    }
}

struct Orchestrator {
    aggregated_changes: AggregatedChanges,
    pending_analysis: Option<PendingAnalysisState>,

    /// Sends messages to the main loop.
    sender: crossbeam_channel::Sender<MainLoopMessage>,
    /// Receives messages from the main loop.
    receiver: crossbeam_channel::Receiver<OrchestratorMessage>,
}

impl Orchestrator {
    fn run(&mut self) {
        while let Ok(message) = self.receiver.recv() {
            match message {
                OrchestratorMessage::Run => {
                    self.pending_analysis = None;
                    self.sender.send(MainLoopMessage::CheckProgram).unwrap();
                }

                OrchestratorMessage::CheckProgramStarted { cancellation_token } => {
                    debug_assert!(self.pending_analysis.is_none());

                    self.pending_analysis = Some(PendingAnalysisState { cancellation_token });
                }

                OrchestratorMessage::CheckProgramCompleted(diagnostics) => {
                    self.pending_analysis
                        .take()
                        .expect("Expected a pending analysis.");

                    self.sender
                        .send(MainLoopMessage::CheckCompleted(diagnostics))
                        .unwrap();
                }

                OrchestratorMessage::CheckProgramCancelled => {
                    self.pending_analysis
                        .take()
                        .expect("Expected a pending analysis.");

                    self.debounce_changes();
                }

                OrchestratorMessage::FileChanges(changes) => {
                    // Request cancellation, but wait until all analysis tasks have completed to
                    // avoid stale messages in the next main loop.
                    let pending = if let Some(pending_state) = self.pending_analysis.as_ref() {
                        pending_state.cancellation_token.cancel();
                        true
                    } else {
                        false
                    };

                    self.aggregated_changes.extend(changes);

                    // If there are no pending analysis tasks, apply the file changes. Otherwise
                    // keep running until all file checks have completed.
                    if !pending {
                        self.debounce_changes();
                    }
                }
                OrchestratorMessage::Shutdown => {
                    return self.shutdown();
                }
            }
        }
    }

    fn debounce_changes(&mut self) {
        debug_assert!(self.pending_analysis.is_none());

        loop {
            // Consume possibly incoming file change messages before running a new analysis, but don't wait for more than 100ms.
            crossbeam_channel::select! {
                recv(self.receiver) -> message => {
                    match message {
                        Ok(OrchestratorMessage::Shutdown) => {
                            return self.shutdown();
                        }
                        Ok(OrchestratorMessage::FileChanges(file_changes)) => {
                            self.aggregated_changes.extend(file_changes);
                        }

                        Ok(OrchestratorMessage::CheckProgramStarted {..}| OrchestratorMessage::CheckProgramCompleted(_) | OrchestratorMessage::CheckProgramCancelled) => unreachable!("The program check should be complete at this point."),
                        Ok(OrchestratorMessage::Run) => unreachable!("The orchestrator is already running."),

                        Err(_) => {
                            // There are no more senders, no point in waiting for more messages
                            return;
                        }
                    }
                },
                default(std::time::Duration::from_millis(100)) => {
                    // No more file changes after 100 ms, send the changes and schedule a new analysis
                    self.sender.send(MainLoopMessage::ApplyChanges(std::mem::take(&mut self.aggregated_changes))).unwrap();
                    self.sender.send(MainLoopMessage::CheckProgram).unwrap();
                    return;
                }
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn shutdown(&self) {
        tracing::trace!("Shutting down orchestrator.");
    }
}

#[derive(Debug)]
struct PendingAnalysisState {
    cancellation_token: CancellationTokenSource,
}

/// Message sent from the orchestrator to the main loop.
#[derive(Debug)]
enum MainLoopMessage {
    CheckProgram,
    CheckCompleted(Vec<String>),
    ApplyChanges(AggregatedChanges),
    Exit,
}

#[derive(Debug)]
enum OrchestratorMessage {
    Run,
    Shutdown,

    CheckProgramStarted {
        cancellation_token: CancellationTokenSource,
    },
    CheckProgramCompleted(Vec<String>),
    CheckProgramCancelled,

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
                        // Deletion after creations means that ruff never saw the file.
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
