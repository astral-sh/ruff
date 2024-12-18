use std::process::{ExitCode, Termination};
use std::sync::Mutex;

use anyhow::{anyhow, Context};
use clap::Parser;
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;
use python_version::PythonVersion;
use red_knot_python_semantic::SitePackages;
use red_knot_server::run_server;
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::watch;
use red_knot_workspace::watch::WorkspaceWatcher;
use red_knot_workspace::workspace::settings::Configuration;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};
use salsa::plumbing::ZalsaDatabase;

use crate::logging::{setup_tracing, Verbosity};

mod logging;
mod python_version;
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

    /// Run the command within the given project directory.
    ///
    /// All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory,
    /// as will the project's virtual environment (`.venv`) unless the `venv-path` option is set.
    ///
    /// Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.
    #[arg(long, value_name = "PROJECT")]
    project: Option<SystemPathBuf>,

    /// Path to the virtual environment the project uses.
    ///
    /// If provided, red-knot will use the `site-packages` directory of this virtual environment
    /// to resolve type information for the project's third-party dependencies.
    #[arg(long, value_name = "PATH")]
    venv_path: Option<SystemPathBuf>,

    /// Custom directory to use for stdlib typeshed stubs.
    #[arg(long, value_name = "PATH", alias = "custom-typeshed-dir")]
    typeshed: Option<SystemPathBuf>,

    /// Additional path to use as a module-resolution source (can be passed multiple times).
    #[arg(long, value_name = "PATH")]
    extra_search_path: Option<Vec<SystemPathBuf>>,

    /// Python version to assume when resolving types.
    #[arg(long, value_name = "VERSION", alias = "target-version")]
    python_version: Option<PythonVersion>,

    #[clap(flatten)]
    verbosity: Verbosity,

    /// Run in watch mode by re-running whenever files change.
    #[arg(long, short = 'W')]
    watch: bool,
}

impl Args {
    fn to_configuration(&self, cli_cwd: &SystemPath) -> Configuration {
        let mut configuration = Configuration::default();

        if let Some(python_version) = self.python_version {
            configuration.python_version = Some(python_version.into());
        }

        if let Some(venv_path) = &self.venv_path {
            configuration.search_paths.site_packages = Some(SitePackages::Derived {
                venv_path: SystemPath::absolute(venv_path, cli_cwd),
            });
        }

        if let Some(typeshed) = &self.typeshed {
            configuration.search_paths.typeshed = Some(SystemPath::absolute(typeshed, cli_cwd));
        }

        if let Some(extra_search_paths) = &self.extra_search_path {
            configuration.search_paths.extra_paths = extra_search_paths
                .iter()
                .map(|path| Some(SystemPath::absolute(path, cli_cwd)))
                .collect();
        }

        configuration
    }
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Start the language server
    Server,
}

use oxidd::bdd::BDDFunction;
use oxidd::tdd::TDDFunction;
use oxidd::BooleanFunction;
use oxidd::ManagerRef;
use oxidd::TVLFunction;

#[allow(clippy::print_stdout, clippy::unnecessary_wraps, clippy::print_stderr)]
pub fn main() -> ExitStatus {
    let mgr = oxidd::bdd::new_manager(24, 24, 1);
    let (x, y, z) = mgr.with_manager_exclusive(|mgr| {
        (
            BDDFunction::new_var(mgr).unwrap(),
            BDDFunction::new_var(mgr).unwrap(),
            BDDFunction::new_var(mgr).unwrap(),
        )
    });
    let res = x.and(&y).unwrap().or(&z).unwrap();
    dbg!(res.eval([(&x, false), (&y, true), (&z, false)]));
    panic!("FOO");
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
    let args = Args::parse_from(std::env::args());

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
                    "The current working directory `{}` contains non-Unicode characters. Red Knot only supports Unicode paths.",
                    path.display()
                )
            })?
    };

    let cwd = args
        .project
        .as_ref()
        .map(|cwd| {
            if cwd.as_std_path().is_dir() {
                Ok(SystemPath::absolute(cwd, &cli_base_path))
            } else {
                Err(anyhow!("Provided project path `{cwd}` is not a directory"))
            }
        })
        .transpose()?
        .unwrap_or_else(|| cli_base_path.clone());

    let system = OsSystem::new(cwd.clone());
    let cli_configuration = args.to_configuration(&cwd);
    let workspace_metadata = WorkspaceMetadata::discover(
        system.current_directory(),
        &system,
        Some(&cli_configuration),
    )?;

    // TODO: Use the `program_settings` to compute the key for the database's persistent
    //   cache and load the cache if it exists.
    let mut db = RootDatabase::new(workspace_metadata, system)?;

    let (main_loop, main_loop_cancellation_token) = MainLoop::new(cli_configuration);

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

    cli_configuration: Configuration,
}

impl MainLoop {
    fn new(cli_configuration: Configuration) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
                cli_configuration,
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
                    let db = db.clone();
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
                        #[allow(clippy::print_stdout)]
                        for diagnostic in result {
                            println!("{}", diagnostic.display(db));
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
                    db.apply_changes(changes, Some(&self.cli_configuration));
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
    CheckCompleted {
        result: Vec<Box<dyn Diagnostic>>,
        revision: u64,
    },
    ApplyChanges(Vec<watch::ChangeEvent>),
    Exit,
}
