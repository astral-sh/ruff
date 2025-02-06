use std::io::{self, BufWriter, Write};
use std::process::{ExitCode, Termination};

use anyhow::Result;
use std::sync::Mutex;

use crate::args::{Args, CheckCommand, Command};
use crate::logging::{setup_metrics, setup_tracing};
use anyhow::{anyhow, Context};
use clap::Parser;
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;
use red_knot_project::metadata::options::Options;
use red_knot_project::watch;
use red_knot_project::watch::ProjectWatcher;
use red_knot_project::{ProjectDatabase, ProjectMetadata};
use red_knot_server::run_server;
use ruff_db::diagnostic::{Diagnostic, Severity};
use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};
use salsa::plumbing::ZalsaDatabase;

mod args;
mod logging;
mod python_version;
mod verbosity;
mod version;

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
    let args = Args::parse_from(std::env::args());

    match args.command {
        Command::Server => run_server().map(|()| ExitStatus::Success),
        Command::Check(check_args) => run_check(check_args),
        Command::Version => version().map(|()| ExitStatus::Success),
    }
}

pub(crate) fn version() -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    let version_info = crate::version::version();
    writeln!(stdout, "red knot {}", &version_info)?;
    Ok(())
}

fn run_check(args: CheckCommand) -> anyhow::Result<ExitStatus> {
    let verbosity = args.verbosity.level();
    countme::enable(verbosity.is_trace());
    let _guard = setup_tracing(verbosity)?;
    setup_metrics(args.metrics.as_ref());

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

    let system = OsSystem::new(cwd);
    let watch = args.watch;
    let exit_zero = args.exit_zero;
    let min_error_severity = if args.error_on_warning {
        Severity::Warning
    } else {
        Severity::Error
    };

    let cli_options = args.into_options();
    let mut workspace_metadata = ProjectMetadata::discover(system.current_directory(), &system)?;
    workspace_metadata.apply_cli_options(cli_options.clone());

    let mut db = ProjectDatabase::new(workspace_metadata, system)?;

    let (main_loop, main_loop_cancellation_token) = MainLoop::new(cli_options, min_error_severity);

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

    if exit_zero {
        Ok(ExitStatus::Success)
    } else {
        Ok(exit_status)
    }
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
    watcher: Option<ProjectWatcher>,

    cli_options: Options,

    /// The minimum severity to consider an error when deciding the exit status.
    ///
    /// TODO(micha): Get from the terminal settings.
    min_error_severity: Severity,
}

impl MainLoop {
    fn new(
        cli_options: Options,
        min_error_severity: Severity,
    ) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
                cli_options,
                min_error_severity,
            },
            MainLoopCancellationToken { sender },
        )
    }

    fn watch(mut self, db: &mut ProjectDatabase) -> anyhow::Result<ExitStatus> {
        tracing::debug!("Starting watch mode");
        let sender = self.sender.clone();
        let watcher = watch::directory_watcher(move |event| {
            sender.send(MainLoopMessage::ApplyChanges(event)).unwrap();
        })?;

        self.watcher = Some(ProjectWatcher::new(watcher, db));

        self.run(db);

        Ok(ExitStatus::Success)
    }

    fn run(mut self, db: &mut ProjectDatabase) -> ExitStatus {
        self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();

        let result = self.main_loop(db);

        tracing::debug!("Exiting main loop");

        result
    }

    fn main_loop(&mut self, db: &mut ProjectDatabase) -> ExitStatus {
        // Schedule the first check.
        tracing::debug!("Starting main loop");

        let mut revision = 0u64;

        while let Ok(message) = self.receiver.recv() {
            match message {
                MainLoopMessage::CheckWorkspace => {
                    let db = db.clone();
                    let sender = self.sender.clone();

                    // Spawn a new task that checks the project. This needs to be done in a separate thread
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
                    let failed = result
                        .iter()
                        .any(|diagnostic| diagnostic.severity() >= self.min_error_severity);

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
                        return if failed {
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
                    db.apply_changes(changes, Some(&self.cli_options));
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
