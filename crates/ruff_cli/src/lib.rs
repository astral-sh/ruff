use std::io::{self, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::channel;

use anyhow::Result;
use clap::CommandFactory;
use notify::{recommended_watcher, RecursiveMode, Watcher};

use ruff::logging::{set_up_logging, LogLevel};
use ruff::settings::types::SerializationFormat;
use ruff::settings::{flags, CliSettings};
use ruff::{fs, warn_user_once};

use crate::args::{Args, CheckArgs, Command};
use crate::printer::{Flags as PrinterFlags, Printer};

pub mod args;
mod cache;
mod commands;
mod diagnostics;
mod panic;
mod printer;
mod resolve;

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
{}: `ruff` crashed. This indicates a bug in `ruff`. If you could open an issue at:

https://github.com/charliermarsh/ruff/issues/new?title=%5BPanic%5D

quoting the executed command, along with the relevant file contents and `pyproject.toml` settings, we'd be very appreciative!
"#,
                    "error".red().bold(),
                );
            }
            default_panic_hook(info);
        }));
    }

    // Enabled ANSI colors on Windows 10.
    #[cfg(windows)]
    assert!(colored::control::set_virtual_terminal(true).is_ok());

    let log_level: LogLevel = (&log_level_args).into();
    set_up_logging(&log_level)?;

    match command {
        Command::Rule { rule, format } => commands::rule::rule(rule, format)?,
        Command::Config { option } => return Ok(commands::config::config(option.as_deref())),
        Command::Linter { format } => commands::linter::linter(format)?,
        Command::Clean => commands::clean::clean(log_level)?,
        Command::GenerateShellCompletion { shell } => {
            shell.generate(&mut Args::command(), &mut io::stdout());
        }
        Command::Check(args) => return check(args, log_level),
    }

    Ok(ExitStatus::Success)
}

fn check(args: CheckArgs, log_level: LogLevel) -> Result<ExitStatus> {
    let (cli, overrides) = args.partition();

    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside of the hierarchy.
    let pyproject_strategy = resolve::resolve(
        cli.isolated,
        cli.config.as_deref(),
        &overrides,
        cli.stdin_filename.as_deref(),
    )?;

    if cli.show_settings {
        commands::show_settings::show_settings(&cli.files, &pyproject_strategy, &overrides)?;
        return Ok(ExitStatus::Success);
    }
    if cli.show_files {
        commands::show_files::show_files(&cli.files, &pyproject_strategy, &overrides)?;
        return Ok(ExitStatus::Success);
    }

    // Extract options that are included in `Settings`, but only apply at the top
    // level.
    let CliSettings {
        fix,
        fix_only,
        format,
        show_fixes,
        update_check,
        ..
    } = pyproject_strategy.top_level_settings().cli.clone();

    // Autofix rules are as follows:
    // - If `--fix` or `--fix-only` is set, always apply fixes to the filesystem (or
    //   print them to stdout, if we're reading from stdin).
    // - Otherwise, if `--format json` is set, generate the fixes (so we print them
    //   out as part of the JSON payload), but don't write them to disk.
    // - If `--diff` or `--fix-only` are set, don't print any violations (only
    //   fixes).
    // TODO(charlie): Consider adding ESLint's `--fix-dry-run`, which would generate
    // but not apply fixes. That would allow us to avoid special-casing JSON
    // here.
    let autofix = if cli.diff {
        flags::FixMode::Diff
    } else if fix || fix_only {
        flags::FixMode::Apply
    } else if matches!(format, SerializationFormat::Json) {
        flags::FixMode::Generate
    } else {
        flags::FixMode::None
    };
    let cache = !cli.no_cache;
    let noqa = !cli.ignore_noqa;
    let mut printer_flags = PrinterFlags::empty();
    if !(cli.diff || fix_only) {
        printer_flags |= PrinterFlags::SHOW_VIOLATIONS;
    }
    if show_fixes {
        printer_flags |= PrinterFlags::SHOW_FIXES;
    }

    #[cfg(debug_assertions)]
    if cache {
        // `--no-cache` doesn't respect code changes, and so is often confusing during
        // development.
        warn_user_once!("Detected debug build without --no-cache.");
    }

    if cli.add_noqa {
        if !matches!(autofix, flags::FixMode::None) {
            warn_user_once!("--fix is incompatible with --add-noqa.");
        }
        let modifications =
            commands::add_noqa::add_noqa(&cli.files, &pyproject_strategy, &overrides)?;
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
        if !matches!(autofix, flags::FixMode::None) {
            warn_user_once!("--fix is unsupported in watch mode.");
        }
        if format != SerializationFormat::Text {
            warn_user_once!("--format 'text' is used in watch mode.");
        }

        // Perform an initial run instantly.
        Printer::clear_screen()?;
        printer.write_to_user("Starting linter in watch mode...\n");

        let messages = commands::run::run(
            &cli.files,
            &pyproject_strategy,
            &overrides,
            cache.into(),
            noqa.into(),
            flags::FixMode::None,
        )?;
        printer.write_continuously(&messages)?;

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = recommended_watcher(tx)?;
        for file in &cli.files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        loop {
            match rx.recv() {
                Ok(event) => {
                    let paths = event?.paths;
                    let py_changed = paths.iter().any(|path| {
                        path.extension()
                            .map(|ext| ext == "py" || ext == "pyi")
                            .unwrap_or_default()
                    });
                    if py_changed {
                        Printer::clear_screen()?;
                        printer.write_to_user("File change detected...\n");

                        let messages = commands::run::run(
                            &cli.files,
                            &pyproject_strategy,
                            &overrides,
                            cache.into(),
                            noqa.into(),
                            flags::FixMode::None,
                        )?;
                        printer.write_continuously(&messages)?;
                    }
                }
                Err(err) => return Err(err.into()),
            }
        }
    } else {
        let is_stdin = cli.files == vec![PathBuf::from("-")];

        // Generate lint violations.
        let diagnostics = if is_stdin {
            commands::run_stdin::run_stdin(
                cli.stdin_filename.map(fs::normalize_path).as_deref(),
                &pyproject_strategy,
                &overrides,
                noqa.into(),
                autofix,
            )?
        } else {
            commands::run::run(
                &cli.files,
                &pyproject_strategy,
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
                printer.write_statistics(&diagnostics)?;
            } else {
                let mut stdout = BufWriter::new(io::stdout().lock());
                printer.write_once(&diagnostics, &mut stdout)?;
            }
        }

        if update_check {
            warn_user_once!(
                "update-check has been removed; setting it will cause an error in a future \
                 version."
            );
        }

        if !cli.exit_zero {
            if cli.diff || fix_only {
                if !diagnostics.fixed.is_empty() {
                    return Ok(ExitStatus::Failure);
                }
            } else if cli.exit_non_zero_on_fix {
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
    Ok(ExitStatus::Success)
}
