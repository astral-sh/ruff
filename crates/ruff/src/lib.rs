#![allow(clippy::print_stdout)]

use std::fs::File;
use std::io::{self, stdout, BufWriter, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;

use anyhow::Result;
use clap::CommandFactory;
use colored::Colorize;
use log::warn;
use notify::{recommended_watcher, RecursiveMode, Watcher};

use args::{GlobalConfigArgs, ServerCommand};
use ruff_linter::logging::{set_up_logging, LogLevel};
use ruff_linter::settings::flags::FixMode;
use ruff_linter::settings::types::OutputFormat;
use ruff_linter::{fs, warn_user, warn_user_once};
use ruff_workspace::Settings;

use crate::args::{
    AnalyzeCommand, AnalyzeGraphCommand, Args, CheckCommand, Command, FormatCommand,
};
use crate::printer::{Flags as PrinterFlags, Printer};

pub mod args;
mod cache;
mod commands;
mod diagnostics;
mod printer;
pub mod resolve;
mod stdin;
mod version;

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ChangeKind {
    Configuration,
    SourceFile,
}

/// Return the [`ChangeKind`] based on the list of modified file paths.
///
/// Returns `None` if no relevant changes were detected.
fn change_detected(event: &notify::Event) -> Option<ChangeKind> {
    // If any `.toml` files were modified, return `ChangeKind::Configuration`. Otherwise, return
    // `ChangeKind::SourceFile` if any `.py`, `.pyi`, `.pyw`, or `.ipynb` files were modified.
    let mut source_file = false;

    if event.kind.is_access() || event.kind.is_other() {
        return None;
    }

    if event.need_rescan() {
        return Some(ChangeKind::Configuration);
    }
    for path in &event.paths {
        if let Some(suffix) = path.extension() {
            match suffix.to_str() {
                Some("toml") => {
                    return Some(ChangeKind::Configuration);
                }
                Some("py" | "pyi" | "pyw" | "ipynb") => source_file = true,
                _ => {}
            }
        }
    }
    if source_file {
        return Some(ChangeKind::SourceFile);
    }
    None
}

/// Returns true if the command should read from standard input.
fn is_stdin(files: &[PathBuf], stdin_filename: Option<&Path>) -> bool {
    // If the user provided a `--stdin-filename`, always read from standard input.
    if stdin_filename.is_some() {
        if let Some(file) = files.iter().find(|file| file.as_path() != Path::new("-")) {
            warn_user_once!(
                "Ignoring file {} in favor of standard input.",
                file.display()
            );
        }
        return true;
    }

    let [file] = files else {
        return false;
    };
    // If the user provided exactly `-`, read from standard input.
    file == Path::new("-")
}

/// Returns the default set of files if none are provided, otherwise returns provided files.
fn resolve_default_files(files: Vec<PathBuf>, is_stdin: bool) -> Vec<PathBuf> {
    if files.is_empty() {
        if is_stdin {
            vec![Path::new("-").to_path_buf()]
        } else {
            vec![Path::new(".").to_path_buf()]
        }
    } else {
        files
    }
}

pub fn run(
    Args {
        command,
        global_options,
    }: Args,
) -> Result<ExitStatus> {
    {
        let default_panic_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            #[allow(clippy::print_stderr)]
            {
                eprintln!(
                    r#"
{}{} {} If you could open an issue at:

    https://github.com/astral-sh/ruff/issues/new?title=%5BPanic%5D

...quoting the executed command, along with the relevant file contents and `pyproject.toml` settings, we'd be very appreciative!
"#,
                    "error".red().bold(),
                    ":".bold(),
                    "Ruff crashed.".bold(),
                );
            }
            default_panic_hook(info);
        }));
    }

    // Don't set up logging for the server command, as it has its own logging setup
    // and setting the global logger can only be done once.
    if !matches!(command, Command::Server { .. }) {
        set_up_logging(global_options.log_level())?;
    }

    match command {
        Command::Version { output_format } => {
            commands::version::version(output_format)?;
            Ok(ExitStatus::Success)
        }
        Command::Rule {
            rule,
            all,
            output_format,
        } => {
            if all {
                commands::rule::rules(output_format)?;
            }
            if let Some(rule) = rule {
                commands::rule::rule(rule, output_format)?;
            }
            Ok(ExitStatus::Success)
        }
        Command::Config {
            option,
            output_format,
        } => {
            commands::config::config(option.as_deref(), output_format)?;
            Ok(ExitStatus::Success)
        }
        Command::Linter { output_format } => {
            commands::linter::linter(output_format)?;
            Ok(ExitStatus::Success)
        }
        Command::Clean => {
            commands::clean::clean(global_options.log_level())?;
            Ok(ExitStatus::Success)
        }
        Command::GenerateShellCompletion { shell } => {
            shell.generate(&mut Args::command(), &mut stdout());
            Ok(ExitStatus::Success)
        }
        Command::Check(args) => check(args, global_options),
        Command::Format(args) => format(args, global_options),
        Command::Server(args) => server(args),
        Command::Analyze(AnalyzeCommand::Graph(args)) => analyze_graph(args, global_options),
    }
}

fn format(args: FormatCommand, global_options: GlobalConfigArgs) -> Result<ExitStatus> {
    let (cli, config_arguments) = args.partition(global_options)?;

    if is_stdin(&cli.files, cli.stdin_filename.as_deref()) {
        commands::format_stdin::format_stdin(&cli, &config_arguments)
    } else {
        commands::format::format(cli, &config_arguments)
    }
}

fn analyze_graph(
    args: AnalyzeGraphCommand,
    global_options: GlobalConfigArgs,
) -> Result<ExitStatus> {
    let (cli, config_arguments) = args.partition(global_options)?;

    commands::analyze_graph::analyze_graph(cli, &config_arguments)
}

fn server(args: ServerCommand) -> Result<ExitStatus> {
    let four = NonZeroUsize::new(4).unwrap();

    // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .max(four);
    commands::server::run_server(worker_threads, args.resolve_preview())
}

pub fn check(args: CheckCommand, global_options: GlobalConfigArgs) -> Result<ExitStatus> {
    let (cli, config_arguments) = args.partition(global_options)?;

    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside of the hierarchy.
    let pyproject_config = resolve::resolve(&config_arguments, cli.stdin_filename.as_deref())?;

    let mut writer: Box<dyn Write> = match cli.output_file {
        Some(path) if !cli.watch => {
            colored::control::set_override(false);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = File::create(path)?;
            Box::new(BufWriter::new(file))
        }
        _ => Box::new(BufWriter::new(io::stdout())),
    };
    let stderr_writer = Box::new(BufWriter::new(io::stderr()));

    let is_stdin = is_stdin(&cli.files, cli.stdin_filename.as_deref());
    let files = resolve_default_files(cli.files, is_stdin);

    if cli.show_settings {
        commands::show_settings::show_settings(
            &files,
            &pyproject_config,
            &config_arguments,
            &mut writer,
        )?;
        return Ok(ExitStatus::Success);
    }
    if cli.show_files {
        commands::show_files::show_files(
            &files,
            &pyproject_config,
            &config_arguments,
            &mut writer,
        )?;
        return Ok(ExitStatus::Success);
    }

    // Extract options that are included in `Settings`, but only apply at the top
    // level.
    let Settings {
        fix,
        fix_only,
        unsafe_fixes,
        output_format,
        show_fixes,
        ..
    } = pyproject_config.settings;

    // Fix rules are as follows:
    // - By default, generate all fixes, but don't apply them to the filesystem.
    // - If `--fix` or `--fix-only` is set, apply applicable fixes to the filesystem (or
    //   print them to stdout, if we're reading from stdin).
    // - If `--diff` or `--fix-only` are set, don't print any violations (only applicable fixes)
    // - By default, applicable fixes only include [`Applicability::Automatic`], but if
    //   `--unsafe-fixes` is set, then [`Applicability::Suggested`] fixes are included.

    let fix_mode = if cli.diff {
        FixMode::Diff
    } else if fix || fix_only {
        FixMode::Apply
    } else {
        FixMode::Generate
    };

    let cache = !cli.no_cache;
    let noqa = !cli.ignore_noqa;
    let mut printer_flags = PrinterFlags::empty();
    if !(cli.diff || fix_only) {
        printer_flags |= PrinterFlags::SHOW_VIOLATIONS;
    }
    if show_fixes {
        printer_flags |= PrinterFlags::SHOW_FIX_SUMMARY;
    }

    #[cfg(debug_assertions)]
    if cache {
        // `--no-cache` doesn't respect code changes, and so is often confusing during
        // development.
        warn_user!("Detected debug build without --no-cache.");
    }

    if cli.add_noqa {
        if !fix_mode.is_generate() {
            warn_user!("--fix is incompatible with --add-noqa.");
        }
        let modifications =
            commands::add_noqa::add_noqa(&files, &pyproject_config, &config_arguments)?;
        if modifications > 0 && config_arguments.log_level >= LogLevel::Default {
            let s = if modifications == 1 { "" } else { "s" };
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Added {modifications} noqa directive{s}.");
            }
        }
        return Ok(ExitStatus::Success);
    }

    let printer = Printer::new(
        output_format,
        config_arguments.log_level,
        fix_mode,
        unsafe_fixes,
        printer_flags,
    );

    // the settings should already be combined with the CLI overrides at this point
    // TODO(jane): let's make this `PreviewMode`
    // TODO: this should reference the global preview mode once https://github.com/astral-sh/ruff/issues/8232
    //   is resolved.
    let preview = pyproject_config.settings.linter.preview.is_enabled();

    if cli.watch {
        if output_format != OutputFormat::default() {
            warn_user!(
                "`--output-format {}` is always used in watch mode.",
                OutputFormat::default()
            );
        }

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = recommended_watcher(tx)?;
        for file in &files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }
        if let Some(file) = pyproject_config.path.as_ref() {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        // Perform an initial run instantly.
        Printer::clear_screen()?;
        printer.write_to_user("Starting linter in watch mode...\n");

        let messages = commands::check::check(
            &files,
            &pyproject_config,
            &config_arguments,
            cache.into(),
            noqa.into(),
            fix_mode,
            unsafe_fixes,
        )?;
        printer.write_continuously(&mut writer, &messages, preview)?;

        // In watch mode, we may need to re-resolve the configuration.
        // TODO(charlie): Re-compute other derivative values, like the `printer`.
        let mut pyproject_config = pyproject_config;

        loop {
            match rx.recv() {
                Ok(event) => {
                    let Some(change_kind) = change_detected(&event?) else {
                        continue;
                    };

                    if matches!(change_kind, ChangeKind::Configuration) {
                        pyproject_config =
                            resolve::resolve(&config_arguments, cli.stdin_filename.as_deref())?;
                    }
                    Printer::clear_screen()?;
                    printer.write_to_user("File change detected...\n");

                    let messages = commands::check::check(
                        &files,
                        &pyproject_config,
                        &config_arguments,
                        cache.into(),
                        noqa.into(),
                        fix_mode,
                        unsafe_fixes,
                    )?;
                    printer.write_continuously(&mut writer, &messages, preview)?;
                }
                Err(err) => return Err(err.into()),
            }
        }
    } else {
        // Generate lint violations.
        let diagnostics = if is_stdin {
            commands::check_stdin::check_stdin(
                cli.stdin_filename.map(fs::normalize_path).as_deref(),
                &pyproject_config,
                &config_arguments,
                noqa.into(),
                fix_mode,
            )?
        } else {
            commands::check::check(
                &files,
                &pyproject_config,
                &config_arguments,
                cache.into(),
                noqa.into(),
                fix_mode,
                unsafe_fixes,
            )?
        };

        // Always try to print violations (though the printer itself may suppress output)
        // If we're writing fixes via stdin, the transformed source code goes to the writer
        // so send the summary to stderr instead
        let mut summary_writer = if is_stdin && matches!(fix_mode, FixMode::Apply | FixMode::Diff) {
            stderr_writer
        } else {
            writer
        };
        if cli.statistics {
            printer.write_statistics(&diagnostics, &mut summary_writer)?;
        } else {
            printer.write_once(&diagnostics, &mut summary_writer)?;
        }

        if !cli.exit_zero {
            if cli.diff {
                // If we're printing a diff, we always want to exit non-zero if there are
                // any fixable violations (since we've printed the diff, but not applied the
                // fixes).
                if !diagnostics.fixed.is_empty() {
                    return Ok(ExitStatus::Failure);
                }
            } else if fix_only {
                // If we're only fixing, we want to exit zero (since we've fixed all fixable
                // violations), unless we're explicitly asked to exit non-zero on fix.
                if cli.exit_non_zero_on_fix {
                    if !diagnostics.fixed.is_empty() {
                        return Ok(ExitStatus::Failure);
                    }
                }
            } else {
                // If we're running the linter (not just fixing), we want to exit non-zero if
                // there are any violations, unless we're explicitly asked to exit zero on
                // fix.
                if cli.exit_non_zero_on_fix {
                    if !diagnostics.fixed.is_empty() || !diagnostics.messages.is_empty() {
                        return Ok(ExitStatus::Failure);
                    }
                } else {
                    if !diagnostics.messages.is_empty() {
                        return Ok(ExitStatus::Failure);
                    }
                }
            }
        }
    }
    Ok(ExitStatus::Success)
}

#[cfg(test)]
mod test_file_change_detector {
    use std::path::PathBuf;

    use crate::{change_detected, ChangeKind};

    #[test]
    fn detect_correct_file_change() {
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/pyproject.toml"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("pyproject.toml"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp1/tmp2/tmp3/pyproject.toml"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/ruff.toml"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/.ruff.toml"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::SourceFile),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/rule.py"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::SourceFile),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/rule.pyi"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("pyproject.toml"),
                    PathBuf::from("tmp/rule.py"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/rule.py"),
                    PathBuf::from("pyproject.toml"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
        assert_eq!(
            None,
            change_detected(&notify::Event {
                kind: notify::EventKind::Create(notify::event::CreateKind::File),
                paths: vec![
                    PathBuf::from("tmp/rule.js"),
                    PathBuf::from("tmp/bin/ruff.rs"),
                ],
                attrs: notify::event::EventAttributes::default(),
            }),
        );
    }
}
