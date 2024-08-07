use std::process::ExitCode;
use std::sync::Mutex;

use clap::Parser;
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;

use red_knot_server::run_server;
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::site_packages::site_packages_dirs_of_venv;
use red_knot_workspace::watch;
use red_knot_workspace::watch::WorkspaceWatcher;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::program::{ProgramSettings, SearchPathSettings};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use target_version::TargetVersion;

use crate::logging::{setup_tracing, Verbosity};

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

#[allow(clippy::print_stdout, clippy::unnecessary_wraps, clippy::print_stderr)]
pub fn main() -> ExitCode {
    match run() {
        Ok(status) => status.into(),
        Err(error) => {
            {
                use std::io::Write;

                // Use `writeln` instead of `eprintln` to avoid panicking when the stderr pipe is broken.
                let mut stderr = std::io::stderr().lock();

                // This communicates that this isn't a linter error but ruff itself hard-errored for
                // some reason (e.g. failed to resolve the configuration)
                writeln!(stderr, "{}", "ruff failed".red().bold()).ok();
                // Currently we generally only see one error, but e.g. with io errors when resolving
                // the configuration it is help to chain errors ("resolving configuration failed" ->
                // "failed to read file: subdir/pyproject.toml")
                for cause in error.chain() {
                    writeln!(stderr, "  {} {cause}", "Cause:".bold()).ok();
                }
            }

            ExitStatus::Error.into()
        }
    }
}

fn run() -> anyhow::Result<ExitStatus> {
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

    if matches!(command, Some(Command::Server)) {
        return run_server().map(|()| ExitStatus::Success);
    }

    let verbosity = verbosity.level();
    countme::enable(verbosity.is_trace());
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

    let (main_loop, main_loop_cancellation_token) = MainLoop::new();

    // Listen to Ctrl+C and abort the watch mode.
    let main_loop_cancellation_token = Mutex::new(Some(main_loop_cancellation_token));
    ctrlc::set_handler(move || {
        let mut lock = main_loop_cancellation_token.lock().unwrap();

        if let Some(token) = lock.take() {
            token.stop();
        }
    })?;

    let exit_status = if watch {
        main_loop.watch(&mut db)?
    } else {
        main_loop.run(&mut db)
    };

    tracing::trace!("Counts for entire CLI run:\n{}", countme::get_all());

    std::mem::forget(db);

    Ok(exit_status)
}

#[derive(Copy, Clone)]
pub enum ExitStatus {
    /// Linting was successful and there were no linting errors.
    Success,
    /// Linting was successful but there were linting errors.
    Failure,
    /// Linting failed.
    Error,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => ExitCode::from(0),
            ExitStatus::Failure => ExitCode::from(1),
            ExitStatus::Error => ExitCode::from(2),
        }
    }
}

struct MainLoop {
    /// Sender that can be used to send messages to the main loop.
    sender: crossbeam_channel::Sender<MainLoopMessage>,

    /// Receiver for the messages sent **to** the main loop.
    receiver: crossbeam_channel::Receiver<MainLoopMessage>,

    /// The file system watcher, if running in watch mode.
    watcher: Option<WorkspaceWatcher>,
}

impl MainLoop {
    fn new() -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
            },
            MainLoopCancellationToken { sender },
        )
    }

    fn watch(mut self, db: &mut RootDatabase) -> anyhow::Result<ExitStatus> {
        tracing::debug!("Starting watch mode");
        let sender = self.sender.clone();
        let watcher = watch::directory_watcher(move |event| {
            sender.send(MainLoopMessage::ApplyChanges(event)).unwrap();
        })?;

        self.watcher = Some(WorkspaceWatcher::new(watcher, db));

        self.run(db);

        Ok(ExitStatus::Success)
    }

    fn run(mut self, db: &mut RootDatabase) -> ExitStatus {
        self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();

        let result = self.main_loop(db);

        tracing::debug!("Exiting main loop");

        result
    }

    #[allow(clippy::print_stderr)]
    fn main_loop(&mut self, db: &mut RootDatabase) -> ExitStatus {
        // Schedule the first check.
        tracing::debug!("Starting main loop");

        let mut revision = 0usize;

        while let Ok(message) = self.receiver.recv() {
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
                    } else {
                        tracing::debug!("Discarding check result for outdated revision: current: {revision}, result revision: {check_revision}");
                    }

                    if self.watcher.is_none() {
                        return if result.is_empty() {
                            ExitStatus::Success
                        } else {
                            ExitStatus::Failure
                        };
                    }

                    tracing::trace!("Counts after last check:\n{}", countme::get_all());
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
                    return ExitStatus::Success;
                }
            }

            tracing::debug!("Waiting for next main loop message.");
        }

        ExitStatus::Success
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
