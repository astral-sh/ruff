use std::num::NonZeroUsize;
use std::sync::Mutex;

use clap::Parser;
use crossbeam::channel as crossbeam_channel;
use red_knot_workspace::site_packages::site_packages_dirs_of_venv;

use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::watch;
use red_knot_workspace::watch::WorkspaceWatcher;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::program::{ProgramSettings, SearchPathSettings};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use target_version::TargetVersion;

use crate::logging::{setup_tracing, Verbosity, VerbosityLevel};

mod logging;
mod target_version;
mod verbosity;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "red-knot",
    about = "An experimental multifile analysis backend for Ruff"
)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    #[arg(
        long,
        help = "Changes the current working directory.",
        long_help = "Changes the current working directory before any specified operations. This affects the workspace and configuration discovery.",
        value_name = "PATH"
    )]
    current_directory: Option<SystemPathBuf>,

    #[arg(
        long,
        help = "Path to the virtual environment the project uses",
        long_help = "\
Path to the virtual environment the project uses. \
If provided, red-knot will use the `site-packages` directory of this virtual environment \
to resolve type information for the project's third-party dependencies.",
        value_name = "PATH"
    )]
    venv_path: Option<SystemPathBuf>,

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

    #[clap(flatten)]
    verbosity: Verbosity,

    #[arg(
        long,
        help = "Run in watch mode by re-running whenever files change",
        short = 'W'
    )]
    watch: bool,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Start the language server
    Server,
}

#[allow(
    clippy::print_stdout,
    clippy::unnecessary_wraps,
    clippy::print_stderr,
    clippy::dbg_macro
)]
pub fn main() -> anyhow::Result<()> {
    let Args {
        command,
        current_directory,
        custom_typeshed_dir,
        extra_search_path: extra_paths,
        venv_path,
        target_version,
        verbosity,
        watch,
    } = Args::parse_from(std::env::args().collect::<Vec<_>>());

    let verbosity = verbosity.level();
    countme::enable(verbosity.is_trace());

    if matches!(command, Some(Command::Server)) {
        let four = NonZeroUsize::new(4).unwrap();

        // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
        let worker_threads = std::thread::available_parallelism()
            .unwrap_or(four)
            .max(four);

        return red_knot_server::Server::new(worker_threads)?.run();
    }

    let _guard = setup_tracing(verbosity)?;

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

    let site_packages = if let Some(venv_path) = venv_path {
        let venv_path = system.canonicalize_path(&venv_path).unwrap_or(venv_path);
        assert!(
            system.is_directory(&venv_path),
            "Provided venv-path {venv_path} is not a directory!"
        );
        site_packages_dirs_of_venv(&venv_path, &system).unwrap()
    } else {
        vec![]
    };

    // TODO: Respect the settings from the workspace metadata. when resolving the program settings.
    let program_settings = ProgramSettings {
        target_version: target_version.into(),
        search_paths: SearchPathSettings {
            extra_paths,
            src_root: workspace_metadata.root().to_path_buf(),
            custom_typeshed: custom_typeshed_dir,
            site_packages,
        },
    };

    // TODO: Use the `program_settings` to compute the key for the database's persistent
    //   cache and load the cache if it exists.
    let mut db = RootDatabase::new(workspace_metadata, program_settings, system);

    let (main_loop, main_loop_cancellation_token) = MainLoop::new(verbosity);

    // Listen to Ctrl+C and abort the watch mode.
    let main_loop_cancellation_token = Mutex::new(Some(main_loop_cancellation_token));
    ctrlc::set_handler(move || {
        let mut lock = main_loop_cancellation_token.lock().unwrap();

        if let Some(token) = lock.take() {
            token.stop();
        }
    })?;

    if watch {
        main_loop.watch(&mut db)?;
    } else {
        main_loop.run(&mut db);
    };

    std::mem::forget(db);

    Ok(())
}

struct MainLoop {
    /// Sender that can be used to send messages to the main loop.
    sender: crossbeam_channel::Sender<MainLoopMessage>,

    /// Receiver for the messages sent **to** the main loop.
    receiver: crossbeam_channel::Receiver<MainLoopMessage>,

    /// The file system watcher, if running in watch mode.
    watcher: Option<WorkspaceWatcher>,

    verbosity: VerbosityLevel,
}

impl MainLoop {
    fn new(verbosity: VerbosityLevel) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
                verbosity,
            },
            MainLoopCancellationToken { sender },
        )
    }

    fn watch(mut self, db: &mut RootDatabase) -> anyhow::Result<()> {
        let sender = self.sender.clone();
        let watcher = watch::directory_watcher(move |event| {
            sender.send(MainLoopMessage::ApplyChanges(event)).unwrap();
        })?;

        self.watcher = Some(WorkspaceWatcher::new(watcher, db));

        let is_trace = self.verbosity.is_trace();
        self.run(db);

        if is_trace {
            eprintln!("Exit");
            eprintln!("{}", countme::get_all());
        }

        Ok(())
    }

    #[allow(clippy::print_stderr)]
    fn run(mut self, db: &mut RootDatabase) {
        // Schedule the first check.
        self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();
        let mut revision = 0usize;

        while let Ok(message) = self.receiver.recv() {
            tracing::trace!("Main Loop: Tick");

            match message {
                MainLoopMessage::CheckWorkspace => {
                    let db = db.snapshot();
                    let sender = self.sender.clone();

                    // Spawn a new task that checks the workspace. This needs to be done in a separate thread
                    // to prevent blocking the main loop here.
                    rayon::spawn(move || {
                        if let Ok(result) = db.check() {
                            // Send the result back to the main loop for printing.
                            sender
                                .send(MainLoopMessage::CheckCompleted { result, revision })
                                .ok();
                        }
                    });
                }

                MainLoopMessage::CheckCompleted {
                    result,
                    revision: check_revision,
                } => {
                    if check_revision == revision {
                        eprintln!("{}", result.join("\n"));

                        if self.verbosity.is_trace() {
                            eprintln!("{}", countme::get_all());
                        }
                    }

                    if self.watcher.is_none() {
                        return;
                    }
                }

                MainLoopMessage::ApplyChanges(changes) => {
                    revision += 1;
                    // Automatically cancels any pending queries and waits for them to complete.
                    db.apply_changes(changes);
                    if let Some(watcher) = self.watcher.as_mut() {
                        watcher.update(db);
                    }
                    self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();
                }
                MainLoopMessage::Exit => {
                    return;
                }
            }
        }
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

/// Message sent from the orchestrator to the main loop.
#[derive(Debug)]
enum MainLoopMessage {
    CheckWorkspace,
    CheckCompleted {
        result: Vec<String>,
        revision: usize,
    },
    ApplyChanges(Vec<watch::ChangeEvent>),
    Exit,
}
