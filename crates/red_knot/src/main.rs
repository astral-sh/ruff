use std::io::{self, stdout, BufWriter, Write};
use std::process::{ExitCode, Termination};

use anyhow::Result;
use std::sync::Mutex;

use crate::args::{Args, CheckCommand, Command, TerminalColor};
use crate::logging::setup_tracing;
use anyhow::{anyhow, Context};
use clap::Parser;
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;
use red_knot_project::metadata::options::Options;
use red_knot_project::watch::ProjectWatcher;
use red_knot_project::{watch, Db};
use red_knot_project::{ProjectDatabase, ProjectMetadata};
use red_knot_server::run_server;
use ruff_db::diagnostic::{DisplayDiagnosticConfig, OldDiagnosticTrait, Severity};
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use salsa::plumbing::ZalsaDatabase;

mod args;
mod logging;
mod python_version;
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
            // Exit "gracefully" on broken pipe errors.
            //
            // See: https://github.com/BurntSushi/ripgrep/blob/bf63fe8f258afc09bae6caa48f0ae35eaf115005/crates/core/main.rs#L47C1-L61C14
            if let Some(ioerr) = cause.downcast_ref::<io::Error>() {
                if ioerr.kind() == io::ErrorKind::BrokenPipe {
                    return ExitStatus::Success;
                }
            }

            writeln!(stderr, "  {} {cause}", "Cause:".bold()).ok();
        }

        ExitStatus::Error
    })
}

fn run() -> anyhow::Result<ExitStatus> {
    let args = wild::args_os();
    let args = argfile::expand_args_from(args, argfile::parse_fromfile, argfile::PREFIX)
        .context("Failed to read CLI arguments from file")?;
    let args = Args::parse_from(args);

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
    set_colored_override(args.color);

    let verbosity = args.verbosity.level();
    countme::enable(verbosity.is_trace());
    let _guard = setup_tracing(verbosity)?;

    tracing::debug!("Version: {}", version::version());

    // The base path to which all CLI arguments are relative to.
    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd)
            .map_err(|path| {
                anyhow!(
                    "The current working directory `{}` contains non-Unicode characters. Red Knot only supports Unicode paths.",
                    path.display()
                )
            })?
    };

    let project_path = args
        .project
        .as_ref()
        .map(|project| {
            if project.as_std_path().is_dir() {
                Ok(SystemPath::absolute(project, &cwd))
            } else {
                Err(anyhow!(
                    "Provided project path `{project}` is not a directory"
                ))
            }
        })
        .transpose()?
        .unwrap_or_else(|| cwd.clone());

    let check_paths: Vec<_> = args
        .paths
        .iter()
        .map(|path| SystemPath::absolute(path, &cwd))
        .collect();

    let system = OsSystem::new(cwd);
    let watch = args.watch;
    let exit_zero = args.exit_zero;

    let cli_options = args.into_options();
    let mut project_metadata = ProjectMetadata::discover(&project_path, &system)?;
    project_metadata.apply_cli_options(cli_options.clone());
    project_metadata.apply_configuration_files(&system)?;

    let mut db = ProjectDatabase::new(project_metadata, system)?;

    if !check_paths.is_empty() {
        db.project().set_included_paths(&mut db, check_paths);
    }

    let (main_loop, main_loop_cancellation_token) = MainLoop::new(cli_options);

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
        main_loop.run(&mut db)?
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
}

impl MainLoop {
    fn new(cli_options: Options) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
                cli_options,
            },
            MainLoopCancellationToken { sender },
        )
    }

    fn watch(mut self, db: &mut ProjectDatabase) -> Result<ExitStatus> {
        tracing::debug!("Starting watch mode");
        let sender = self.sender.clone();
        let watcher = watch::directory_watcher(move |event| {
            sender.send(MainLoopMessage::ApplyChanges(event)).unwrap();
        })?;

        self.watcher = Some(ProjectWatcher::new(watcher, db));

        self.run(db)?;

        Ok(ExitStatus::Success)
    }

    fn run(mut self, db: &mut ProjectDatabase) -> Result<ExitStatus> {
        self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();

        let result = self.main_loop(db);

        tracing::debug!("Exiting main loop");

        result
    }

    fn main_loop(&mut self, db: &mut ProjectDatabase) -> Result<ExitStatus> {
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
                    let terminal_settings = db.project().settings(db).terminal();
                    let display_config = DisplayDiagnosticConfig::default()
                        .format(terminal_settings.output_format)
                        .color(colored::control::SHOULD_COLORIZE.should_colorize());

                    let min_error_severity = if terminal_settings.error_on_warning {
                        Severity::Warning
                    } else {
                        Severity::Error
                    };

                    if check_revision == revision {
                        if db.project().files(db).is_empty() {
                            tracing::warn!("No python files found under the given path(s)");
                        }

                        let mut stdout = stdout().lock();

                        if result.is_empty() {
                            writeln!(stdout, "All checks passed!")?;

                            if self.watcher.is_none() {
                                return Ok(ExitStatus::Success);
                            }
                        } else {
                            let mut failed = false;
                            let diagnostics_count = result.len();

                            for diagnostic in result {
                                writeln!(stdout, "{}", diagnostic.display(db, &display_config))?;

                                failed |= diagnostic.severity() >= min_error_severity;
                            }

                            writeln!(
                                stdout,
                                "Found {} diagnostic{}",
                                diagnostics_count,
                                if diagnostics_count > 1 { "s" } else { "" }
                            )?;

                            if self.watcher.is_none() {
                                return Ok(if failed {
                                    ExitStatus::Failure
                                } else {
                                    ExitStatus::Success
                                });
                            }
                        }
                    } else {
                        tracing::debug!(
                            "Discarding check result for outdated revision: current: {revision}, result revision: {check_revision}"
                        );
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
                    return Ok(ExitStatus::Success);
                }
            }

            tracing::debug!("Waiting for next main loop message.");
        }

        Ok(ExitStatus::Success)
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
        /// The diagnostics that were found during the check.
        result: Vec<Box<dyn OldDiagnosticTrait>>,
        revision: u64,
    },
    ApplyChanges(Vec<watch::ChangeEvent>),
    Exit,
}

fn set_colored_override(color: Option<TerminalColor>) {
    let Some(color) = color else {
        return;
    };

    match color {
        TerminalColor::Auto => {
            colored::control::unset_override();
        }
        TerminalColor::Always => {
            colored::control::set_override(true);
        }
        TerminalColor::Never => {
            colored::control::set_override(false);
        }
    }
}
