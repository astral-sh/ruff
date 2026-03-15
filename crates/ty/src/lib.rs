mod args;
mod coverage;
mod logging;
mod printer;
mod python_version;
mod version;

use std::fmt::Write;
use std::process::{ExitCode, Termination};
use std::sync::Mutex;

use anyhow::Result;
use anyhow::{Context, anyhow};
use clap::{CommandFactory, Parser};
use colored::Colorize;
use crossbeam::channel as crossbeam_channel;
use rayon::ThreadPoolBuilder;
use ruff_db::cancellation::{Canceled, CancellationToken, CancellationTokenSource};
use ruff_db::diagnostic::{
    Diagnostic, DiagnosticId, DisplayDiagnosticConfig, DisplayDiagnostics, Severity,
};
use ruff_db::files::File;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ruff_db::{STACK_SIZE, max_parallelism};
use salsa::Database;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::settings::TerminalSettings;
use ty_project::watch::ProjectWatcher;
use ty_project::{CollectReporter, Db, suppress_all_diagnostics, watch};
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_python_semantic::coverage::{
    CoverageStats, FileCoverageDetails, coverage_details as compute_coverage_details,
};
use ty_server::run_server;
use ty_static::EnvVars;

use crate::args::{CheckCommand, Command, CoverageCommand, TerminalColor, VersionFormat};
use crate::logging::{VerbosityLevel, setup_tracing};
use crate::printer::Printer;
pub use args::Cli;

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
        Command::Coverage(coverage_args) => run_coverage(coverage_args),
        Command::Version { output_format } => version(output_format).map(|()| ExitStatus::Success),
        Command::GenerateShellCompletion { shell } => {
            use std::io::stdout;

            shell.generate(&mut Cli::command(), &mut stdout());
            Ok(ExitStatus::Success)
        }
    }
}

pub(crate) fn version(output_format: VersionFormat) -> Result<()> {
    let mut stdout = Printer::default().stream_for_requested_summary().lock();
    let version_info = crate::version::version();

    match output_format {
        VersionFormat::Text => {
            writeln!(stdout, "ty {}", &version_info)?;
        }
        VersionFormat::Json => {
            serde_json::to_writer_pretty(&mut stdout, &version_info)?;
        }
    }
    Ok(())
}

fn load_project(
    cwd: &SystemPath,
    paths: &[SystemPathBuf],
    project: Option<&SystemPathBuf>,
    config_file: Option<SystemPathBuf>,
    options: ty_project::metadata::options::Options,
    system: OsSystem,
    verbosity: VerbosityLevel,
) -> anyhow::Result<(ProjectDatabase, ProjectOptionsOverrides, Vec<SystemPathBuf>)> {
    let project_path = project
        .map(|p| {
            if p.as_std_path().is_dir() {
                Ok(SystemPath::absolute(p, cwd))
            } else {
                Err(anyhow!("Provided project path `{p}` is not a directory"))
            }
        })
        .transpose()?
        .unwrap_or_else(|| cwd.to_path_buf());

    let absolute_paths: Vec<_> = paths
        .iter()
        .map(|path| SystemPath::absolute(path, cwd))
        .collect();

    let mut project_metadata = match &config_file {
        Some(config_file) => {
            ProjectMetadata::from_config_file(config_file.clone(), &project_path, &system)?
        }
        None => ProjectMetadata::discover(&project_path, &system)?,
    };

    project_metadata.apply_configuration_files(&system)?;

    let project_options_overrides = ProjectOptionsOverrides::new(config_file, options);
    project_metadata.apply_overrides(&project_options_overrides);

    let mut db = ProjectDatabase::new(project_metadata, system)?;
    let project = db.project();

    project.set_verbose(&mut db, verbosity >= VerbosityLevel::Verbose);

    if !absolute_paths.is_empty() {
        project.set_included_paths(&mut db, absolute_paths.clone());
    }

    Ok((db, project_options_overrides, absolute_paths))
}

/// Returns the current working directory as a [`SystemPathBuf`].
fn current_working_directory() -> anyhow::Result<SystemPathBuf> {
    let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
    SystemPathBuf::from_path_buf(cwd).map_err(|path| {
        anyhow!(
            "The current working directory `{}` contains non-Unicode characters. ty only supports Unicode paths.",
            path.display()
        )
    })
}

/// Creates the main loop, installs the Ctrl+C handler, and runs (or watches).
fn run_main_loop(
    db: &mut ProjectDatabase,
    project_options_overrides: ProjectOptionsOverrides,
    mode: MainLoopMode,
    printer: Printer,
    watch: bool,
) -> anyhow::Result<ExitStatus> {
    let (main_loop, main_loop_cancellation_token) =
        MainLoop::new(mode, project_options_overrides, printer);

    let main_loop_cancellation_token = Mutex::new(Some(main_loop_cancellation_token));
    ctrlc::set_handler(move || {
        let mut lock = main_loop_cancellation_token.lock().unwrap();
        if let Some(token) = lock.take() {
            token.stop();
        }
    })?;

    if watch {
        main_loop.watch(db)
    } else {
        main_loop.run(db)
    }
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

    let cwd = current_working_directory()?;

    let mode = if args.add_ignore {
        MainLoopMode::AddIgnore
    } else {
        MainLoopMode::Check
    };

    let system = OsSystem::new(&cwd);
    let watch = args.watch;
    let exit_zero = args.exit_zero;
    let force_exclude = args.force_exclude();
    let config_file = args
        .config_file
        .as_ref()
        .map(|path| SystemPath::absolute(path, &cwd));
    // Extract paths and project before consuming args with into_options().
    let paths = args.paths.clone();
    let project = args.project.clone();
    let options = args.into_options();

    let (mut db, project_options_overrides, _) = load_project(
        &cwd,
        &paths,
        project.as_ref(),
        config_file,
        options,
        system,
        verbosity,
    )?;

    db.project().set_force_exclude(&mut db, force_exclude);

    let exit_status = run_main_loop(&mut db, project_options_overrides, mode, printer, watch)?;

    let mut stdout = printer.stream_for_requested_summary().lock();
    match std::env::var(EnvVars::TY_MEMORY_REPORT).as_deref() {
        Ok("short") => write!(stdout, "{}", db.salsa_memory_dump().display_short())?,
        Ok("full") => write!(stdout, "{}", db.salsa_memory_dump().display_full())?,
        Ok("json") => writeln!(stdout, "{}", db.salsa_memory_dump().to_json())?,
        Ok(other) => {
            tracing::warn!(
                "Unknown value for `TY_MEMORY_REPORT`: `{other}`. Valid values are `short`, `full`, and `json`."
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

fn run_coverage(args: CoverageCommand) -> anyhow::Result<ExitStatus> {
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    set_colored_override(args.color);

    let verbosity = args.verbosity.level();
    let _guard = setup_tracing(verbosity, args.color.unwrap_or_default())?;

    let cwd = current_working_directory()?;

    let system = OsSystem::new(&cwd);
    let config_file = args
        .config_file
        .as_ref()
        .map(|path| SystemPath::absolute(path, &cwd));
    // Extract paths and project before consuming args with into_options().
    let paths = args.paths.clone();
    let project = args.project.clone();
    let no_progress = args.no_progress;
    let mode = MainLoopMode::Coverage {
        show_todo: args.todo,
        html_path: args.html.clone(),
        cwd: cwd.clone(),
    };
    let options = args.into_options();

    let (mut db, project_options_overrides, resolved_paths) = load_project(
        &cwd,
        &paths,
        project.as_ref(),
        config_file,
        options,
        system,
        verbosity,
    )?;

    // Verify that each explicitly provided path exists.
    for path in &resolved_paths {
        if !path.as_std_path().exists() {
            return Err(anyhow!("path does not exist: {path}"));
        }
    }

    let printer = Printer::new(verbosity, no_progress);
    let exit_status = run_main_loop(&mut db, project_options_overrides, mode, printer, false)?;

    std::mem::forget(db);

    Ok(exit_status)
}

fn render_coverage(
    db: &ProjectDatabase,
    mut per_file: Vec<(SystemPathBuf, File, FileCoverageDetails)>,
    cwd: &SystemPath,
    show_todo: bool,
    html_path: Option<&SystemPath>,
) -> anyhow::Result<()> {
    use std::io::Write as _;

    // Sort by combined line-level imprecision descending, then by path for stable output.
    // When todo reporting is off, todo lines are folded into dynamic for display, so include
    // them in the sort key too so the table order matches the displayed percentages.
    let sort_key = |s: &CoverageStats| -> f64 {
        let dynamic = if show_todo {
            s.dynamic
        } else {
            s.dynamic + s.todo
        };
        let total = s.total();
        if total == 0 {
            0.0
        } else {
            (dynamic + s.imprecise) as f64 / total as f64 * 100.0
        }
    };
    per_file.sort_by(|(path_a, _, details_a), (path_b, _, details_b)| {
        sort_key(&details_b.stats)
            .partial_cmp(&sort_key(&details_a.stats))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| path_a.cmp(path_b))
    });

    let total = per_file
        .iter()
        .fold(CoverageStats::default(), |acc, (_, _, d)| {
            acc.merge(d.stats)
        });

    // Compute common directory prefix to strip from displayed paths.
    let prefix = common_path_prefix(per_file.iter().map(|(p, _, _)| p.as_path()));

    let display_path =
        |p: &SystemPath| -> String { p.strip_prefix(&prefix).unwrap_or(p).as_str().to_owned() };

    let mut stdout = std::io::stdout().lock();

    {
        use coverage::table::{Align, AsciiTable, Column};

        let mut columns = vec![
            Column::new("File", Align::Left),
            Column::new("Lines", Align::Right),
            Column::new("Precise", Align::Right),
            Column::new("Imprecise", Align::Right),
            Column::new("Dynamic", Align::Right),
        ];
        if show_todo {
            columns.push(Column::new("Todo", Align::Right));
        }
        columns.push(Column::new("Empty", Align::Right));

        let mut tbl = AsciiTable::new(columns);

        for (path, _, d) in &per_file {
            let ls = &d.stats;
            let n = ls.total();
            let mut row = vec![
                display_path(path),
                n.to_string(),
                fmt_count_pct(ls.precise, n),
                fmt_count_pct(ls.imprecise, n),
                fmt_count_pct(ls.dynamic, n),
            ];
            if show_todo {
                row.push(fmt_count_pct(ls.todo, n));
            }
            row.push(fmt_count_pct(ls.empty, n));
            tbl.push_row(row);
        }

        let tl = &total;
        let tn = tl.total();
        let mut footer = vec![
            "Total".to_owned(),
            tn.to_string(),
            fmt_count_pct(tl.precise, tn),
            fmt_count_pct(tl.imprecise, tn),
            fmt_count_pct(tl.dynamic, tn),
        ];
        if show_todo {
            footer.push(fmt_count_pct(tl.todo, tn));
        }
        footer.push(fmt_count_pct(tl.empty, tn));
        tbl.set_footer(footer);

        tbl.render(&mut stdout)?;
    }

    if let Some(html_out) = html_path {
        let abs = SystemPath::absolute(html_out, cwd);
        coverage::html::write_html_report(&abs, &per_file, &prefix, db, show_todo)?;
        writeln!(stdout, "HTML report written to {abs}")?;
    }

    Ok(())
}

/// Formats a count with its percentage of a total as `"N (X%)"`.
#[expect(clippy::cast_precision_loss)]
fn fmt_count_pct(count: u64, total: u64) -> String {
    if total == 0 {
        return format!("{count} (0%)");
    }
    let pct = (count as f64 / total as f64 * 100.0).round();
    format!("{count} ({pct:.0}%)")
}

/// Returns the longest common directory prefix of the given paths.
/// If the paths share no common ancestor (or the slice is empty), returns an empty path.
fn common_path_prefix<'a>(mut paths: impl Iterator<Item = &'a SystemPath>) -> SystemPathBuf {
    let Some(first) = paths.next() else {
        return SystemPathBuf::new();
    };

    // Start from the first path's parent directory.
    let mut prefix = first
        .parent()
        .map(SystemPath::to_path_buf)
        .unwrap_or_default();

    for path in paths {
        // Walk up until `path` starts with `prefix`.
        while !path.starts_with(&prefix) {
            let Some(parent) = prefix.parent().map(SystemPath::to_path_buf) else {
                return SystemPathBuf::new();
            };
            prefix = parent;
        }
    }

    prefix
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
    mode: MainLoopMode,

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
        mode: MainLoopMode,
        project_options_overrides: ProjectOptionsOverrides,
        printer: Printer,
    ) -> (Self, MainLoopCancellationToken) {
        let (sender, receiver) = crossbeam_channel::bounded(10);

        let cancellation_token_source = CancellationTokenSource::new();
        let cancellation_token = cancellation_token_source.token();

        (
            Self {
                mode,
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
        // Schedule the first workspace analysis (check or coverage).
        tracing::debug!("Starting main loop");

        let mut revision = 0u64;

        while let Ok(message) = self.receiver.recv() {
            match message {
                MainLoopMessage::CheckWorkspace => {
                    let db = db.clone();
                    let sender = self.sender.clone();

                    if matches!(self.mode, MainLoopMode::Coverage { .. }) {
                        // Coverage mode: compute per-file details in a background thread.
                        rayon::spawn(move || {
                            let bar = indicatif::ProgressBar::hidden();
                            let bar_for_cancel = bar.clone();

                            match salsa::Cancelled::catch(|| {
                                let files = db.project().files(&db);
                                bar.set_length(files.len() as u64);
                                bar.set_message("Coverage");
                                bar.set_style(
                                    indicatif::ProgressStyle::with_template(
                                        "{msg:8.dim} {bar:60.green/dim} {pos}/{len} files",
                                    )
                                    .unwrap()
                                    .progress_chars("--"),
                                );
                                bar.set_draw_target(self.printer.progress_target());

                                let result = files
                                    .iter()
                                    .filter_map(|file| {
                                        let path = file.path(&db).as_system_path()?.to_path_buf();
                                        let details = compute_coverage_details(&db, *file);
                                        bar.inc(1);
                                        Some((path, *file, details))
                                    })
                                    .collect::<Vec<_>>();
                                bar.finish_and_clear();
                                result
                            }) {
                                Ok(per_file) => {
                                    sender
                                        .send(MainLoopMessage::CoverageCompleted {
                                            per_file,
                                            revision,
                                        })
                                        .unwrap();
                                }
                                Err(cancelled) => {
                                    bar_for_cancel.finish_and_clear();
                                    tracing::debug!(
                                        "Coverage computation cancelled: {cancelled:?}"
                                    );
                                }
                            }
                        });
                    } else {
                        // Check/AddIgnore mode: run diagnostics in a background thread.
                        rayon::spawn(move || {
                            let mut reporter = IndicatifReporter::from(self.printer);
                            let bar = reporter.bar.clone();

                            match salsa::Cancelled::catch(|| {
                                db.check_with_reporter(&mut reporter);
                                reporter.bar.finish_and_clear();
                                reporter.collector.into_sorted(&db)
                            }) {
                                Ok(result) => {
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
                }

                MainLoopMessage::CheckCompleted {
                    result,
                    revision: check_revision,
                } => {
                    if check_revision != revision {
                        tracing::debug!(
                            "Discarding check result for outdated revision: current: {revision}, result revision: {check_revision}"
                        );
                        continue;
                    }

                    if db.project().files(db).is_empty() {
                        tracing::warn!("No python files found under the given path(s)");
                    }

                    let result = match &self.mode {
                        MainLoopMode::Check => {
                            // TODO: We should have an official flag to silence workspace diagnostics.
                            if std::env::var("TY_MEMORY_REPORT").as_deref() == Ok("json") {
                                return Ok(ExitStatus::Success);
                            }

                            self.write_diagnostics(db, &result)?;

                            if self.cancellation_token.is_cancelled() {
                                Err(Canceled)
                            } else {
                                Ok(result)
                            }
                        }
                        MainLoopMode::AddIgnore => {
                            if let Ok(result) =
                                suppress_all_diagnostics(db, result, &self.cancellation_token)
                            {
                                self.write_diagnostics(db, &result.diagnostics)?;

                                let terminal_settings = db.project().settings(db).terminal();
                                let is_human_readable =
                                    terminal_settings.output_format.is_human_readable();

                                if is_human_readable {
                                    writeln!(
                                        self.printer.stream_for_failure_summary(),
                                        "Added {} ignore comment{}",
                                        result.count,
                                        if result.count > 1 { "s" } else { "" }
                                    )?;
                                }

                                Ok(result.diagnostics)
                            } else {
                                Err(Canceled)
                            }
                        }
                        // Coverage mode never sends CheckCompleted; ignore stale messages.
                        MainLoopMode::Coverage { .. } => continue,
                    };

                    let exit_status = match result.as_deref() {
                        Ok([]) => ExitStatus::Success,
                        Ok(diagnostics) => {
                            let terminal_settings = db.project().settings(db).terminal();
                            exit_status_from_diagnostics(diagnostics, terminal_settings)
                        }
                        Err(Canceled) => ExitStatus::Success,
                    };

                    if exit_status.is_internal_error() {
                        tracing::warn!(
                            "A fatal error occurred while checking some files. Not all project files were analyzed. See the diagnostics list above for details."
                        );
                    }

                    if self.watcher.is_some() {
                        continue;
                    }

                    return Ok(exit_status);
                }

                MainLoopMessage::CoverageCompleted {
                    per_file,
                    revision: check_revision,
                } => {
                    if check_revision != revision {
                        tracing::debug!(
                            "Discarding coverage result for outdated revision: current: {revision}, result revision: {check_revision}"
                        );
                        continue;
                    }

                    if let MainLoopMode::Coverage {
                        show_todo,
                        html_path,
                        cwd,
                    } = &self.mode
                    {
                        render_coverage(db, per_file, cwd, *show_todo, html_path.as_deref())?;
                    }

                    if self.watcher.is_some() {
                        continue;
                    }

                    return Ok(ExitStatus::Success);
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

    fn write_diagnostics(
        &self,
        db: &ProjectDatabase,
        diagnostics: &[Diagnostic],
    ) -> anyhow::Result<()> {
        let terminal_settings = db.project().settings(db).terminal();
        let is_human_readable = terminal_settings.output_format.is_human_readable();

        match diagnostics {
            [] if is_human_readable => {
                writeln!(
                    self.printer.stream_for_success_summary(),
                    "{}",
                    "All checks passed!".green().bold()
                )?;
            }
            diagnostics => {
                let diagnostics_count = diagnostics.len();

                let mut stdout = self.printer.stream_for_details().lock();

                // Only render diagnostics if they're going to be displayed, since doing
                // so is expensive.
                if stdout.is_enabled() {
                    let display_config = DisplayDiagnosticConfig::new("ty")
                        .format(terminal_settings.output_format.into())
                        .color(colored::control::SHOULD_COLORIZE.should_colorize())
                        .with_cancellation_token(Some(self.cancellation_token.clone()))
                        .show_fix_diff(true);

                    write!(
                        stdout,
                        "{}",
                        DisplayDiagnostics::new(db, &display_config, diagnostics)
                    )?;
                }

                if !self.cancellation_token.is_cancelled() && is_human_readable {
                    writeln!(
                        self.printer.stream_for_failure_summary(),
                        "Found {} diagnostic{}",
                        diagnostics_count,
                        if diagnostics_count > 1 { "s" } else { "" }
                    )?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
enum MainLoopMode {
    Check,
    AddIgnore,
    Coverage {
        show_todo: bool,
        html_path: Option<SystemPathBuf>,
        cwd: SystemPathBuf,
    },
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
    CoverageCompleted {
        per_file: Vec<(SystemPathBuf, File, FileCoverageDetails)>,
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
        .stack_size(STACK_SIZE)
        .build_global()
        .unwrap();
}
