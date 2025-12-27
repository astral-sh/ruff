mod args;
mod logging;
mod printer;
mod python_version;
mod version;

pub use args::Cli;
use ty_project::metadata::settings::TerminalSettings;
use ty_static::EnvVars;

use std::fmt::Write;
use std::process::{ExitCode, Termination};
use std::sync::Mutex;

use anyhow::Result;

use crate::args::{CheckCommand, Command, TerminalColor};
use crate::logging::{VerbosityLevel, setup_tracing};
use crate::printer::Printer;
use anyhow::{Context, anyhow};
use clap::{CommandFactory, Parser};
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;
use rayon::ThreadPoolBuilder;
use ruff_db::cancellation::{CancellationToken, CancellationTokenSource};
use ruff_db::diagnostic::{
    Diagnostic, DiagnosticId, DisplayDiagnosticConfig, DisplayDiagnostics, Severity,
};
use ruff_db::files::File;
use ruff_db::max_parallelism;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use salsa::Database;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::watch::ProjectWatcher;
use ty_project::{CollectReporter, Db, watch};
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_server::run_server;

pub fn run() -> anyhow::Result<ExitStatus> {
    setup_rayon();
    ruff_db::set_program_version(crate::version::version().to_string()).unwrap();

    let args = wild::args_os();
    let args = argfile::expand_args_from(args, argfile::parse_fromfile, argfile::PREFIX)
        .context("Failed to read CLI arguments from file")?;
    let args = Cli::parse_from(args);

    match args.command {
        Command::Server => run_server().map(|()| ExitStatus::Success),
        Command::Check(check_args) => run_check(check_args),
        Command::Version => version().map(|()| ExitStatus::Success),
        Command::GenerateShellCompletion { shell } => {
            use std::io::stdout;

            shell.generate(&mut Cli::command(), &mut stdout());
            Ok(ExitStatus::Success)
        }
    }
}

pub(crate) fn version() -> Result<()> {
    let mut stdout = Printer::default().stream_for_requested_summary().lock();
    let version_info = crate::version::version();
    writeln!(stdout, "ty {}", &version_info)?;
    Ok(())
}

fn run_check(args: CheckCommand) -> anyhow::Result<ExitStatus> {
    // Enabled ANSI colors on Windows 10.
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    set_colored_override(args.color);

    let verbosity = args.verbosity.level();
    let _guard = setup_tracing(verbosity, args.color.unwrap_or_default())?;

    let printer = Printer::new(verbosity, args.no_progress);

    tracing::debug!("Version: {}", version::version());

    // The base path to which all CLI arguments are relative to.
    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd)
            .map_err(|path| {
                anyhow!(
                    "The current working directory `{}` contains non-Unicode characters. ty only supports Unicode paths.",
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

    let system = OsSystem::new(&cwd);
    let watch = args.watch;
    let exit_zero = args.exit_zero;
    let config_file = args
        .config_file
        .as_ref()
        .map(|path| SystemPath::absolute(path, &cwd));
    let force_exclude = args.force_exclude();

    let mut project_metadata = match &config_file {
        Some(config_file) => {
            ProjectMetadata::from_config_file(config_file.clone(), &project_path, &system)?
        }
        None => ProjectMetadata::discover(&project_path, &system)?,
    };

    project_metadata.apply_configuration_files(&system)?;

    let project_options_overrides = ProjectOptionsOverrides::new(config_file, args.into_options());
    project_metadata.apply_overrides(&project_options_overrides);

    let mut db = ProjectDatabase::new(project_metadata, system)?;
    let project = db.project();

    project.set_verbose(&mut db, verbosity >= VerbosityLevel::Verbose);
    project.set_force_exclude(&mut db, force_exclude);

    if !check_paths.is_empty() {
        project.set_included_paths(&mut db, check_paths);
    }

    let (main_loop, main_loop_cancellation_token) =
        MainLoop::new(project_options_overrides, printer);

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

    let mut stdout = printer.stream_for_requested_summary().lock();
    match std::env::var(EnvVars::TY_MEMORY_REPORT).as_deref() {
        Ok("short") => write!(stdout, "{}", db.salsa_memory_dump().display_short())?,
        Ok("mypy_primer") => write!(stdout, "{}", db.salsa_memory_dump().display_mypy_primer())?,
        Ok("full") => {
            write!(stdout, "{}", db.salsa_memory_dump().display_full())?;
        }
        Ok(other) => {
            tracing::warn!(
                "Unknown value for `TY_MEMORY_REPORT`: `{other}`. Valid values are `short`, `mypy_primer`, and `full`."
            );
        }
        Err(_) => {}
    }

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

    /// Checking failed due to an invocation error (e.g. the current directory no longer exists, incorrect CLI arguments, ...)
    Error = 2,

    /// Internal ty error (panic, or any other error that isn't due to the user using the
    /// program incorrectly or transient environment errors).
    InternalError = 101,
}

impl ExitStatus {
    pub const fn is_internal_error(self) -> bool {
        matches!(self, ExitStatus::InternalError)
    }
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

    /// Interface for displaying information to the user.
    printer: Printer,

    project_options_overrides: ProjectOptionsOverrides,

    /// Cancellation token that gets set by Ctrl+C.
    /// Used for long-running operations on the main thread. Operations on background threads
    /// use Salsa's cancellation mechanism.
    cancellation_token: CancellationToken,
}

impl MainLoop {
    fn new(
        project_options_overrides: ProjectOptionsOverrides,
        printer: Printer,
    ) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        let cancellation_token_source = CancellationTokenSource::new();
        let cancellation_token = cancellation_token_source.token();

        (
            Self {
                sender: sender.clone(),
                receiver,
                watcher: None,
                project_options_overrides,
                printer,
                cancellation_token,
            },
            MainLoopCancellationToken {
                sender,
                source: cancellation_token_source,
            },
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

    fn run(self, db: &mut ProjectDatabase) -> Result<ExitStatus> {
        self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();

        let result = self.main_loop(db);

        tracing::debug!("Exiting main loop");

        result
    }

    fn main_loop(mut self, db: &mut ProjectDatabase) -> Result<ExitStatus> {
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
                        let mut reporter = IndicatifReporter::from(self.printer);
                        let bar = reporter.bar.clone();

                        match salsa::Cancelled::catch(|| {
                            db.check_with_reporter(&mut reporter);
                            reporter.bar.finish_and_clear();
                            reporter.collector.into_sorted(&db)
                        }) {
                            Ok(result) => {
                                // Send the result back to the main loop for printing.
                                sender
                                    .send(MainLoopMessage::CheckCompleted { result, revision })
                                    .unwrap();
                            }
                            Err(cancelled) => {
                                bar.finish_and_clear();
                                tracing::debug!("Check has been cancelled: {cancelled:?}");
                            }
                        }
                    });
                }

                MainLoopMessage::CheckCompleted {
                    result,
                    revision: check_revision,
                } => {
                    let terminal_settings = db.project().settings(db).terminal();
                    let display_config = DisplayDiagnosticConfig::default()
                        .format(terminal_settings.output_format.into())
                        .color(colored::control::SHOULD_COLORIZE.should_colorize())
                        .with_cancellation_token(Some(self.cancellation_token.clone()))
                        .show_fix_diff(true);

                    if check_revision == revision {
                        if db.project().files(db).is_empty() {
                            tracing::warn!("No python files found under the given path(s)");
                        }

                        // TODO: We should have an official flag to silence workspace diagnostics.
                        if std::env::var("TY_MEMORY_REPORT").as_deref() == Ok("mypy_primer") {
                            return Ok(ExitStatus::Success);
                        }

                        let is_human_readable = terminal_settings.output_format.is_human_readable();

                        if result.is_empty() {
                            if is_human_readable {
                                writeln!(
                                    self.printer.stream_for_success_summary(),
                                    "{}",
                                    "All checks passed!".green().bold()
                                )?;
                            }

                            if self.watcher.is_none() {
                                return Ok(ExitStatus::Success);
                            }
                        } else {
                            let diagnostics_count = result.len();

                            let mut stdout = self.printer.stream_for_details().lock();
                            let exit_status =
                                exit_status_from_diagnostics(&result, terminal_settings);

                            // Only render diagnostics if they're going to be displayed, since doing
                            // so is expensive.
                            if stdout.is_enabled() {
                                write!(
                                    stdout,
                                    "{}",
                                    DisplayDiagnostics::new(db, &display_config, &result)
                                )?;
                            }

                            if !self.cancellation_token.is_cancelled() {
                                if is_human_readable {
                                    writeln!(
                                        self.printer.stream_for_failure_summary(),
                                        "Found {} diagnostic{}",
                                        diagnostics_count,
                                        if diagnostics_count > 1 { "s" } else { "" }
                                    )?;
                                }

                                if exit_status.is_internal_error() {
                                    tracing::warn!(
                                        "A fatal error occurred while checking some files. Not all project files were analyzed. See the diagnostics list above for details."
                                    );
                                }
                            }

                            if self.watcher.is_none() {
                                return Ok(exit_status);
                            }
                        }
                    } else {
                        tracing::debug!(
                            "Discarding check result for outdated revision: current: {revision}, result revision: {check_revision}"
                        );
                    }
                }

                MainLoopMessage::ApplyChanges(changes) => {
                    Printer::clear_screen()?;

                    revision += 1;
                    // Automatically cancels any pending queries and waits for them to complete.
                    db.apply_changes(changes, Some(&self.project_options_overrides));
                    if let Some(watcher) = self.watcher.as_mut() {
                        watcher.update(db);
                    }

                    self.sender.send(MainLoopMessage::CheckWorkspace).unwrap();
                }
                MainLoopMessage::Exit => {
                    // Cancel any pending queries and wait for them to complete.
                    db.trigger_cancellation();
                    return Ok(ExitStatus::Success);
                }
            }

            tracing::debug!("Waiting for next main loop message.");
        }

        Ok(ExitStatus::Success)
    }
}

fn exit_status_from_diagnostics(
    diagnostics: &[Diagnostic],
    terminal_settings: &TerminalSettings,
) -> ExitStatus {
    if diagnostics.is_empty() {
        return ExitStatus::Success;
    }

    let mut max_severity = Severity::Info;
    let mut io_error = false;

    for diagnostic in diagnostics {
        max_severity = max_severity.max(diagnostic.severity());
        io_error = io_error || matches!(diagnostic.id(), DiagnosticId::Io);
    }

    if !max_severity.is_fatal() && io_error {
        return ExitStatus::Error;
    }

    match max_severity {
        Severity::Info => ExitStatus::Success,
        Severity::Warning => {
            if terminal_settings.error_on_warning {
                ExitStatus::Failure
            } else {
                ExitStatus::Success
            }
        }
        Severity::Error => ExitStatus::Failure,
        Severity::Fatal => ExitStatus::InternalError,
    }
}

/// A progress reporter for `ty check`.
struct IndicatifReporter {
    collector: CollectReporter,

    /// A reporter that is ready, containing a progress bar to report to.
    ///
    /// Initialization of the bar is deferred to [`ty_project::ProgressReporter::set_files`] so we
    /// do not initialize the bar too early as it may take a while to collect the number of files to
    /// process and we don't want to display an empty "0/0" bar.
    bar: indicatif::ProgressBar,

    printer: Printer,
}

impl From<Printer> for IndicatifReporter {
    fn from(printer: Printer) -> Self {
        Self {
            bar: indicatif::ProgressBar::hidden(),
            collector: CollectReporter::default(),
            printer,
        }
    }
}

impl ty_project::ProgressReporter for IndicatifReporter {
    fn set_files(&mut self, files: usize) {
        self.collector.set_files(files);

        self.bar.set_length(files as u64);
        self.bar.set_message("Checking");
        self.bar.set_style(
            indicatif::ProgressStyle::with_template(
                "{msg:8.dim} {bar:60.green/dim} {pos}/{len} files",
            )
            .unwrap()
            .progress_chars("--"),
        );
        self.bar.set_draw_target(self.printer.progress_target());
    }

    fn report_checked_file(&self, db: &ProjectDatabase, file: File, diagnostics: &[Diagnostic]) {
        self.collector.report_checked_file(db, file, diagnostics);
        self.bar.inc(1);
    }

    fn report_diagnostics(&mut self, db: &ProjectDatabase, diagnostics: Vec<Diagnostic>) {
        self.collector.report_diagnostics(db, diagnostics);
    }
}

#[derive(Debug)]
struct MainLoopCancellationToken {
    sender: crossbeam_channel::Sender<MainLoopMessage>,
    source: CancellationTokenSource,
}

impl MainLoopCancellationToken {
    fn stop(self) {
        self.source.cancel();
        self.sender.send(MainLoopMessage::Exit).unwrap();
    }
}

/// Message sent from the orchestrator to the main loop.
#[derive(Debug)]
enum MainLoopMessage {
    CheckWorkspace,
    CheckCompleted {
        /// The diagnostics that were found during the check.
        result: Vec<Diagnostic>,
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

/// Initializes the global rayon thread pool to never use more than `TY_MAX_PARALLELISM` threads.
fn setup_rayon() {
    ThreadPoolBuilder::default()
        .num_threads(max_parallelism().get())
        // Use a reasonably large stack size to avoid running into stack overflows too easily. The
        // size was chosen in such a way as to still be able to handle large expressions involving
        // binary operators (x + x + … + x) both during the AST walk in semantic index building as
        // well as during type checking. Using this stack size, we can handle handle expressions
        // that are several times larger than the corresponding limits in existing type checkers.
        .stack_size(16 * 1024 * 1024)
        .build_global()
        .unwrap();
}
