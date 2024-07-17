use std::sync::Mutex;

use clap::Parser;
use crossbeam::channel as crossbeam_channel;
use salsa::ParallelDatabase;
use tracing::subscriber::Interest;
use tracing::{Level, Metadata};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::{Layer, Registry};
use tracing_tree::time::Uptime;

use red_knot::db::RootDatabase;
use red_knot::watch::FileWatcher;
use red_knot::watch::FileWatcherChange;
use red_knot::workspace::WorkspaceMetadata;
use ruff_db::program::{ProgramSettings, SearchPathSettings};
use ruff_db::system::{OsSystem, System, SystemPathBuf};

use self::target_version::TargetVersion;

mod target_version;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "red-knot",
    about = "An experimental multifile analysis backend for Ruff"
)]
#[command(version)]
struct Args {
    #[arg(
        long,
        help = "Changes the current working directory.",
        long_help = "Changes the current working directory before any specified operations. This affects the workspace and configuration discovery.",
        value_name = "PATH"
    )]
    current_directory: Option<SystemPathBuf>,

    #[arg(
        long,
        value_name = "DIRECTORY",
        help = "Custom directory to use for stdlib typeshed stubs"
    )]
    custom_typeshed_dir: Option<SystemPathBuf>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Additional path to use as a module-resolution source (can be passed multiple times)"
    )]
    extra_search_path: Vec<SystemPathBuf>,
    #[arg(long, help = "Python version to assume when resolving types", default_value_t = TargetVersion::default(), value_name="VERSION")]
    target_version: TargetVersion,
}

#[allow(
    clippy::print_stdout,
    clippy::unnecessary_wraps,
    clippy::print_stderr,
    clippy::dbg_macro
)]
pub fn main() -> anyhow::Result<()> {
    countme::enable(true);
    setup_tracing();

    let Args {
        current_directory,
        custom_typeshed_dir,
        extra_search_path: extra_paths,
        target_version,
    } = Args::parse_from(std::env::args().collect::<Vec<_>>());

    let cwd = if let Some(cwd) = current_directory {
        let canonicalized = cwd.as_utf8_path().canonicalize_utf8().unwrap();
        SystemPathBuf::from_utf8_path_buf(canonicalized)
    } else {
        let cwd = std::env::current_dir().unwrap();
        SystemPathBuf::from_path_buf(cwd).unwrap()
    };

    let system = OsSystem::new(cwd.clone());
    let workspace_metadata =
        WorkspaceMetadata::from_path(system.current_directory(), &system).unwrap();

    // TODO: Respect the settings from the workspace metadata. when resolving the program settings.
    let program_settings = ProgramSettings {
        target_version: target_version.into(),
        search_paths: SearchPathSettings {
            extra_paths,
            workspace_root: workspace_metadata.root().to_path_buf(),
            custom_typeshed: custom_typeshed_dir,
            site_packages: None,
        },
    };

    // TODO: Use the `program_settings` to compute the key for the database's persistent
    //   cache and load the cache if it exists.
    let mut db = RootDatabase::new(workspace_metadata, program_settings, system);

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
    let mut file_watcher = FileWatcher::new(move |changes| {
        file_changes_notifier.notify(changes);
    })?;

    file_watcher.watch_folder(db.workspace().root(&db).as_std_path())?;

    main_loop.run(&mut db);

    println!("{}", countme::get_all());

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
            receiver: orchestrator_receiver,
            sender: main_loop_sender.clone(),
            revision: 0,
        };

        std::thread::spawn(move || {
            orchestrator.run();
        });

        (
            Self {
                orchestrator_sender,
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

    #[allow(clippy::print_stderr)]
    fn run(self, db: &mut RootDatabase) {
        self.orchestrator_sender
            .send(OrchestratorMessage::Run)
            .unwrap();

        for message in &self.main_loop_receiver {
            tracing::trace!("Main Loop: Tick");

            match message {
                MainLoopMessage::CheckWorkspace { revision } => {
                    let db = db.snapshot();
                    let sender = self.orchestrator_sender.clone();

                    // Spawn a new task that checks the workspace. This needs to be done in a separate thread
                    // to prevent blocking the main loop here.
                    rayon::spawn(move || {
                        if let Ok(result) = db.check() {
                            sender
                                .send(OrchestratorMessage::CheckCompleted {
                                    diagnostics: result,
                                    revision,
                                })
                                .unwrap();
                        }
                    });
                }
                MainLoopMessage::ApplyChanges(changes) => {
                    // Automatically cancels any pending queries and waits for them to complete.
                    db.apply_changes(changes);
                }
                MainLoopMessage::CheckCompleted(diagnostics) => {
                    eprintln!("{}", diagnostics.join("\n"));
                    eprintln!("{}", countme::get_all());
                }
                MainLoopMessage::Exit => {
                    eprintln!("{}", countme::get_all());
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
    fn notify(&self, changes: Vec<FileWatcherChange>) {
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
    /// Sends messages to the main loop.
    sender: crossbeam_channel::Sender<MainLoopMessage>,
    /// Receives messages from the main loop.
    receiver: crossbeam_channel::Receiver<OrchestratorMessage>,
    revision: usize,
}

impl Orchestrator {
    #[allow(clippy::print_stderr)]
    fn run(&mut self) {
        while let Ok(message) = self.receiver.recv() {
            match message {
                OrchestratorMessage::Run => {
                    self.sender
                        .send(MainLoopMessage::CheckWorkspace {
                            revision: self.revision,
                        })
                        .unwrap();
                }

                OrchestratorMessage::CheckCompleted {
                    diagnostics,
                    revision,
                } => {
                    // Only take the diagnostics if they are for the latest revision.
                    if self.revision == revision {
                        self.sender
                            .send(MainLoopMessage::CheckCompleted(diagnostics))
                            .unwrap();
                    } else {
                        tracing::debug!("Discarding diagnostics for outdated revision {revision} (current: {}).", self.revision);
                    }
                }

                OrchestratorMessage::FileChanges(changes) => {
                    // Request cancellation, but wait until all analysis tasks have completed to
                    // avoid stale messages in the next main loop.

                    self.revision += 1;
                    self.debounce_changes(changes);
                }
                OrchestratorMessage::Shutdown => {
                    return self.shutdown();
                }
            }
        }
    }

    fn debounce_changes(&self, mut changes: Vec<FileWatcherChange>) {
        loop {
            // Consume possibly incoming file change messages before running a new analysis, but don't wait for more than 100ms.
            crossbeam_channel::select! {
                recv(self.receiver) -> message => {
                    match message {
                        Ok(OrchestratorMessage::Shutdown) => {
                            return self.shutdown();
                        }
                        Ok(OrchestratorMessage::FileChanges(file_changes)) => {
                            changes.extend(file_changes);
                        }

                        Ok(OrchestratorMessage::CheckCompleted { .. })=> {
                            // disregard any outdated completion message.
                        }
                        Ok(OrchestratorMessage::Run) => unreachable!("The orchestrator is already running."),

                        Err(_) => {
                            // There are no more senders, no point in waiting for more messages
                            return;
                        }
                    }
                },
                default(std::time::Duration::from_millis(10)) => {
                    // No more file changes after 10 ms, send the changes and schedule a new analysis
                    self.sender.send(MainLoopMessage::ApplyChanges(changes)).unwrap();
                    self.sender.send(MainLoopMessage::CheckWorkspace { revision: self.revision}).unwrap();
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

/// Message sent from the orchestrator to the main loop.
#[derive(Debug)]
enum MainLoopMessage {
    CheckWorkspace { revision: usize },
    CheckCompleted(Vec<String>),
    ApplyChanges(Vec<FileWatcherChange>),
    Exit,
}

#[derive(Debug)]
enum OrchestratorMessage {
    Run,
    Shutdown,

    CheckCompleted {
        diagnostics: Vec<String>,
        revision: usize,
    },

    FileChanges(Vec<FileWatcherChange>),
}

fn setup_tracing() {
    let subscriber = Registry::default().with(
        tracing_tree::HierarchicalLayer::default()
            .with_indent_lines(true)
            .with_indent_amount(2)
            .with_bracketed_fields(true)
            .with_thread_ids(true)
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
