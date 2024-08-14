use std::process::{ExitCode, Termination};
use std::sync::Mutex;

use anyhow::{anyhow, Context};
use clap::Parser;
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;
use salsa::plumbing::ZalsaDatabase;

use red_knot_python_semantic::PythonVersion;
use red_knot_server::run_server;
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::watch;
use red_knot_workspace::watch::WorkspaceWatcher;
use red_knot_workspace::workspace::settings::{
    SitePackages, WorkspaceConfiguration, WorkspaceConfigurationTransformer,
};
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};
use target_version::TargetVersion;

use crate::logging::{setup_tracing, Verbosity};

mod logging;
mod target_version;
mod verbosity;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "red-knot",
    about = "An extremely fast Python type checker."
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
    extra_search_path: Option<Vec<SystemPathBuf>>,

    #[arg(
        long,
        help = "Python version to assume when resolving types",
        value_name = "VERSION"
    )]
    target_version: Option<TargetVersion>,

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
pub fn main() -> ExitStatus {
    run().unwrap_or_else(|error| {
        use std::io::Write;

        // Use `writeln` instead of `eprintln` to avoid panicking when the stderr pipe is broken.
        let mut stderr = std::io::stderr().lock();

        // This communicates that this isn't a linter error but Red Knot itself hard-errored for
        // some reason (e.g. failed to resolve the configuration)
        writeln!(stderr, "{}", "Red Knot failed".red().bold()).ok();
        // Currently we generally only see one error, but e.g. with io errors when resolving
        // the configuration it is help to chain errors ("resolving configuration failed" ->
        // "failed to read file: subdir/pyproject.toml")
        for cause in error.chain() {
            writeln!(stderr, "  {} {cause}", "Cause:".bold()).ok();
        }

        ExitStatus::Error
    })
}

fn run() -> anyhow::Result<ExitStatus> {
    let args = Args::parse_from(std::env::args().collect::<Vec<_>>());

    if matches!(args.command, Some(Command::Server)) {
        return run_server().map(|()| ExitStatus::Success);
    }

    let verbosity = args.verbosity.level();
    countme::enable(verbosity.is_trace());
    let _guard = setup_tracing(verbosity)?;

    // The base path to which all CLI arguments are relative to.
    let cli_base_path = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd)
            .map_err(|path| {
                anyhow!(
                    "The current working directory '{}' contains non-unicode characters. Red Knot only supports unicode paths.",
                    path.display()
                )
            })?
    };

    let cwd = args
        .current_directory
        .as_ref()
        .map(|cwd| {
            if cwd.as_std_path().is_dir() {
                Ok(SystemPath::absolute(cwd, &cli_base_path))
            } else {
                Err(anyhow!(
                    "Provided current-directory path '{cwd}' is not a directory."
                ))
            }
        })
        .transpose()?
        .unwrap_or_else(|| cli_base_path.clone());

    let system = OsSystem::new(cwd.clone());
    let transformer = CliConfigurationTransformer::from_cli_arguments(&args, &cli_base_path);
    let workspace_metadata =
        WorkspaceMetadata::from_path(system.current_directory(), &system, &transformer)?;

    // TODO: Use the `program_settings` to compute the key for the database's persistent
    //   cache and load the cache if it exists.
    let mut db = RootDatabase::new(workspace_metadata, system)?;

    let (main_loop, main_loop_cancellation_token) = MainLoop::new(transformer);

    // Listen to Ctrl+C and abort the watch mode.
    let main_loop_cancellation_token = Mutex::new(Some(main_loop_cancellation_token));
    ctrlc::set_handler(move || {
        let mut lock = main_loop_cancellation_token.lock().unwrap();

        if let Some(token) = lock.take() {
            token.stop();
        }
    })?;

    let exit_status = if args.watch {
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
    /// Checking was successful and there were no errors.
    Success = 0,

    /// Checking was successful but there were errors.
    Failure = 1,

    /// Checking failed.
    Error = 2,
}

impl Termination for ExitStatus {
    fn report(self) -> ExitCode {
        ExitCode::from(self as u8)
    }
}

struct MainLoop {
    /// Sender that can be used to send messages to the main loop.
    sender: crossbeam_channel::Sender<MainLoopMessage>,

    /// Receiver for the messages sent **to** the main loop.
    receiver: crossbeam_channel::Receiver<MainLoopMessage>,

    /// The file system watcher, if running in watch mode.
    watcher: Option<WorkspaceWatcher>,

    configuration_transformer: CliConfigurationTransformer,
}

impl MainLoop {
    fn new(
        configuration_transformer: CliConfigurationTransformer,
    ) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
                configuration_transformer,
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

    fn main_loop(&mut self, db: &mut RootDatabase) -> ExitStatus {
        // Schedule the first check.
        tracing::debug!("Starting main loop");

        let mut revision = 0u64;

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
                                .unwrap();
                        }
                    });
                }

                MainLoopMessage::CheckCompleted {
                    result,
                    revision: check_revision,
                } => {
                    let has_diagnostics = !result.is_empty();
                    if check_revision == revision {
                        for diagnostic in result {
                            tracing::error!("{}", diagnostic);
                        }
                    } else {
                        tracing::debug!(
                            "Discarding check result for outdated revision: current: {revision}, result revision: {check_revision}"
                        );
                    }

                    if self.watcher.is_none() {
                        return if has_diagnostics {
                            ExitStatus::Failure
                        } else {
                            ExitStatus::Success
                        };
                    }

                    tracing::trace!("Counts after last check:\n{}", countme::get_all());
                }

                MainLoopMessage::ApplyChanges(changes) => {
                    revision += 1;
                    // Automatically cancels any pending queries and waits for them to complete.
                    db.apply_changes(changes, &self.configuration_transformer);
                    if let Some(watcher) = self.watcher.as_mut() {
                        watcher.update(db);
                    }
                    self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();
                }
                MainLoopMessage::Exit => {
                    // Cancel any pending queries and wait for them to complete.
                    // TODO: Don't use Salsa internal APIs
                    //  [Zulip-Thread](https://salsa.zulipchat.com/#narrow/stream/333573-salsa-3.2E0/topic/Expose.20an.20API.20to.20cancel.20other.20queries)
                    let _ = db.zalsa_mut();
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
    CheckCompleted { result: Vec<String>, revision: u64 },
    ApplyChanges(Vec<watch::ChangeEvent>),
    Exit,
}

#[derive(Debug, Default)]
struct CliConfigurationTransformer {
    venv_path: Option<SystemPathBuf>,
    custom_typeshed_dir: Option<SystemPathBuf>,
    extra_search_paths: Option<Vec<SystemPathBuf>>,
    target_version: Option<PythonVersion>,
}

impl CliConfigurationTransformer {
    fn from_cli_arguments(arguments: &Args, cli_cwd: &SystemPath) -> Self {
        let Args {
            venv_path,
            custom_typeshed_dir,
            extra_search_path,
            target_version,
            ..
        } = arguments;

        let venv_path = venv_path
            .as_deref()
            .map(|path| SystemPath::absolute(path, cli_cwd));

        let custom_typeshed_dir = custom_typeshed_dir
            .as_deref()
            .map(|path| SystemPath::absolute(path, cli_cwd));

        let extra_search_paths = extra_search_path.as_deref().map(|paths| {
            paths
                .iter()
                .map(|path| SystemPath::absolute(path, cli_cwd))
                .collect()
        });

        Self {
            venv_path,
            custom_typeshed_dir,
            extra_search_paths,
            target_version: target_version.map(PythonVersion::from),
        }
    }
}

impl WorkspaceConfigurationTransformer for CliConfigurationTransformer {
    fn transform(&self, mut configuration: WorkspaceConfiguration) -> WorkspaceConfiguration {
        if let Some(venv_path) = &self.venv_path {
            configuration.search_paths.site_packages = Some(SitePackages::Derived {
                venv_path: venv_path.clone(),
            });
        }

        if let Some(custom_typeshed_dir) = &self.custom_typeshed_dir {
            configuration.search_paths.custom_typeshed = Some(custom_typeshed_dir.clone());
        }

        if let Some(extra_search_paths) = &self.extra_search_paths {
            configuration.search_paths.extra_paths = Some(extra_search_paths.clone());
        }

        if let Some(target_version) = self.target_version {
            configuration.target_version = Some(target_version);
        }

        configuration
    }
}
