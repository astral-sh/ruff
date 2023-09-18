use std::fs::File;
use std::io::{self, stdout, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;

use anyhow::Result;
use clap::CommandFactory;
use log::warn;
use notify::{recommended_watcher, RecursiveMode, Watcher};

use ruff::logging::{set_up_logging, LogLevel};
use ruff::settings::types::SerializationFormat;
use ruff::settings::{flags, CliSettings};
use ruff::{fs, warn_user_once};

use crate::args::{Args, CheckCommand, Command, FormatCommand};
use crate::printer::{Flags as PrinterFlags, Printer};

pub mod args;
mod cache;
mod commands;
mod diagnostics;
mod panic;
mod printer;
pub mod resolve;
mod stdin;

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
fn change_detected(paths: &[PathBuf]) -> Option<ChangeKind> {
    // If any `.toml` files were modified, return `ChangeKind::Configuration`. Otherwise, return
    // `ChangeKind::SourceFile` if any `.py`, `.pyi`, `.pyw`, or `.ipynb` files were modified.
    let mut source_file = false;
    for path in paths {
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

pub fn run(
    Args {
        command,
        log_level_args,
    }: Args,
) -> Result<ExitStatus> {
    {
        use colored::Colorize;

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

    // Enabled ANSI colors on Windows 10.
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    let log_level = LogLevel::from(&log_level_args);
    set_up_logging(&log_level)?;

    match command {
        Command::Rule { rule, all, format } => {
            if all {
                commands::rule::rules(format)?;
            }
            if let Some(rule) = rule {
                commands::rule::rule(rule, format)?;
            }
            Ok(ExitStatus::Success)
        }
        Command::Config { option } => {
            commands::config::config(option.as_deref())?;
            Ok(ExitStatus::Success)
        }
        Command::Linter { format } => {
            commands::linter::linter(format)?;
            Ok(ExitStatus::Success)
        }
        Command::Clean => {
            commands::clean::clean(log_level)?;
            Ok(ExitStatus::Success)
        }
        Command::GenerateShellCompletion { shell } => {
            shell.generate(&mut Args::command(), &mut stdout());
            Ok(ExitStatus::Success)
        }
        Command::Check(args) => check(args, log_level),
        Command::Format(args) => format(args, log_level),
    }
}

fn format(args: FormatCommand, log_level: LogLevel) -> Result<ExitStatus> {
    warn_user_once!(
        "`ruff format` is a work-in-progress, subject to change at any time, and intended only for \
        experimentation."
    );

    let (cli, overrides) = args.partition();

    if is_stdin(&cli.files, cli.stdin_filename.as_deref()) {
        commands::format_stdin::format_stdin(&cli, &overrides)
    } else {
        commands::format::format(&cli, &overrides, log_level)
    }
}

pub fn check(args: CheckCommand, log_level: LogLevel) -> Result<ExitStatus> {
    let (cli, overrides) = args.partition();

    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside of the hierarchy.
    let pyproject_config = resolve::resolve(
        cli.isolated,
        cli.config.as_deref(),
        &overrides,
        cli.stdin_filename.as_deref(),
    )?;

    let mut writer: Box<dyn Write> = match cli.output_file {
        Some(path) if !cli.watch => {
            colored::control::set_override(false);
            let file = File::create(path)?;
            Box::new(BufWriter::new(file))
        }
        _ => Box::new(BufWriter::new(io::stdout())),
    };

    if cli.show_settings {
        commands::show_settings::show_settings(
            &cli.files,
            &pyproject_config,
            &overrides,
            &mut writer,
        )?;
        return Ok(ExitStatus::Success);
    }
    if cli.show_files {
        commands::show_files::show_files(&cli.files, &pyproject_config, &overrides, &mut writer)?;
        return Ok(ExitStatus::Success);
    }

    // Extract options that are included in `Settings`, but only apply at the top
    // level.
    let CliSettings {
        fix,
        fix_only,
        format,
        show_fixes,
        show_source,
        ..
    } = pyproject_config.settings.cli;

    // Autofix rules are as follows:
    // - By default, generate all fixes, but don't apply them to the filesystem.
    // - If `--fix` or `--fix-only` is set, always apply fixes to the filesystem (or
    //   print them to stdout, if we're reading from stdin).
    // - If `--diff` or `--fix-only` are set, don't print any violations (only
    //   fixes).
    let autofix = if cli.diff {
        flags::FixMode::Diff
    } else if fix || fix_only {
        flags::FixMode::Apply
    } else {
        flags::FixMode::Generate
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
    if show_source {
        printer_flags |= PrinterFlags::SHOW_SOURCE;
    }
    if cli.ecosystem_ci {
        warn_user_once!(
            "The formatting of fixes emitted by this option is a work-in-progress, subject to \
            change at any time, and intended only for internal use."
        );
        printer_flags |= PrinterFlags::SHOW_FIX_DIFF;
    }

    #[cfg(debug_assertions)]
    if cache {
        // `--no-cache` doesn't respect code changes, and so is often confusing during
        // development.
        warn_user_once!("Detected debug build without --no-cache.");
    }

    if cli.add_noqa {
        if !autofix.is_generate() {
            warn_user_once!("--fix is incompatible with --add-noqa.");
        }
        let modifications =
            commands::add_noqa::add_noqa(&cli.files, &pyproject_config, &overrides)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            let s = if modifications == 1 { "" } else { "s" };
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Added {modifications} noqa directive{s}.");
            }
        }
        return Ok(ExitStatus::Success);
    }

    let printer = Printer::new(format, log_level, autofix, printer_flags);

    if cli.watch {
        if format != SerializationFormat::Text {
            warn_user_once!("--format 'text' is used in watch mode.");
        }

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = recommended_watcher(tx)?;
        for file in &cli.files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }
        if let Some(file) = pyproject_config.path.as_ref() {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        // Perform an initial run instantly.
        Printer::clear_screen()?;
        printer.write_to_user("Starting linter in watch mode...\n");

        let messages = commands::check::check(
            &cli.files,
            &pyproject_config,
            &overrides,
            cache.into(),
            noqa.into(),
            autofix,
        )?;
        printer.write_continuously(&mut writer, &messages)?;

        // In watch mode, we may need to re-resolve the configuration.
        // TODO(charlie): Re-compute other derivative values, like the `printer`.
        let mut pyproject_config = pyproject_config;

        loop {
            match rx.recv() {
                Ok(event) => {
                    let Some(change_kind) = change_detected(&event?.paths) else {
                        continue;
                    };

                    if matches!(change_kind, ChangeKind::Configuration) {
                        pyproject_config = resolve::resolve(
                            cli.isolated,
                            cli.config.as_deref(),
                            &overrides,
                            cli.stdin_filename.as_deref(),
                        )?;
                    }
                    Printer::clear_screen()?;
                    printer.write_to_user("File change detected...\n");

                    let messages = commands::check::check(
                        &cli.files,
                        &pyproject_config,
                        &overrides,
                        cache.into(),
                        noqa.into(),
                        autofix,
                    )?;
                    printer.write_continuously(&mut writer, &messages)?;
                }
                Err(err) => return Err(err.into()),
            }
        }
    } else {
        let is_stdin = is_stdin(&cli.files, cli.stdin_filename.as_deref());

        // Generate lint violations.
        let diagnostics = if is_stdin {
            commands::check_stdin::check_stdin(
                cli.stdin_filename.map(fs::normalize_path).as_deref(),
                &pyproject_config,
                &overrides,
                noqa.into(),
                autofix,
            )?
        } else {
            commands::check::check(
                &cli.files,
                &pyproject_config,
                &overrides,
                cache.into(),
                noqa.into(),
                autofix,
            )?
        };

        // Always try to print violations (the printer itself may suppress output),
        // unless we're writing fixes via stdin (in which case, the transformed
        // source code goes to stdout).
        if !(is_stdin && matches!(autofix, flags::FixMode::Apply | flags::FixMode::Diff)) {
            if cli.statistics {
                printer.write_statistics(&diagnostics, &mut writer)?;
            } else {
                printer.write_once(&diagnostics, &mut writer)?;
            }
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
            change_detected(&[
                PathBuf::from("tmp/pyproject.toml"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&[
                PathBuf::from("pyproject.toml"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&[
                PathBuf::from("tmp1/tmp2/tmp3/pyproject.toml"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&[
                PathBuf::from("tmp/ruff.toml"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&[
                PathBuf::from("tmp/.ruff.toml"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::SourceFile),
            change_detected(&[
                PathBuf::from("tmp/rule.py"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::SourceFile),
            change_detected(&[
                PathBuf::from("tmp/rule.pyi"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&[
                PathBuf::from("pyproject.toml"),
                PathBuf::from("tmp/rule.py"),
            ]),
        );
        assert_eq!(
            Some(ChangeKind::Configuration),
            change_detected(&[
                PathBuf::from("tmp/rule.py"),
                PathBuf::from("pyproject.toml"),
            ]),
        );
        assert_eq!(
            None,
            change_detected(&[
                PathBuf::from("tmp/rule.js"),
                PathBuf::from("tmp/bin/ruff.rs"),
            ]),
        );
    }
}
