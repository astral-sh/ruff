#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;

use ::ruff::autofix::fixer;
use ::ruff::cli::{extract_log_level, Cli};
use ::ruff::logging::{set_up_logging, LogLevel};
use ::ruff::printer::Printer;
use ::ruff::resolver::Strategy;
use ::ruff::settings::configuration::Configuration;
use ::ruff::settings::types::SerializationFormat;
use ::ruff::settings::{pyproject, Settings};
#[cfg(feature = "update-informer")]
use ::ruff::updates;
use ::ruff::{cache, commands};
use anyhow::Result;
use clap::{CommandFactory, Parser};
use colored::Colorize;
use notify::{recommended_watcher, RecursiveMode, Watcher};
use path_absolutize::path_dedot;
use ruff::cli::Overrides;
use ruff::resolver::{resolve_settings, Relativity};

/// Resolve the relevant settings strategy and defaults for the current
/// invocation.
fn resolve(config: Option<PathBuf>, overrides: &Overrides) -> Result<Strategy> {
    if let Some(pyproject) = config {
        // First priority: the user specified a `pyproject.toml` file. Use that
        // `pyproject.toml` for _all_ configuration, and resolve paths relative to the
        // current working directory. (This matches ESLint's behavior.)
        let settings = resolve_settings(&pyproject, &Relativity::Cwd, Some(overrides))?;
        Ok(Strategy::Fixed(settings))
    } else if let Some(pyproject) = pyproject::find_pyproject_toml(path_dedot::CWD.as_path()) {
        // Second priority: find a `pyproject.toml` file in the current working path,
        // and resolve all paths relative to that directory. (With
        // `Strategy::Hierarchical`, we'll end up finding the "closest" `pyproject.toml`
        // file for every Python file later on, so these act as the "default" settings.)
        let settings = resolve_settings(&pyproject, &Relativity::Parent, Some(overrides))?;
        Ok(Strategy::Hierarchical(settings))
    } else if let Some(pyproject) = pyproject::find_user_pyproject_toml() {
        // Third priority: find a user-specific `pyproject.toml`, but resolve all paths
        // relative the current working directory. (With `Strategy::Hierarchical`, we'll
        // end up the "closest" `pyproject.toml` file for every Python file later on, so
        // these act as the "default" settings.)
        let settings = resolve_settings(&pyproject, &Relativity::Cwd, Some(overrides))?;
        Ok(Strategy::Hierarchical(settings))
    } else {
        // Fallback: load Ruff's default settings, and resolve all paths relative to the
        // current working directory. (With `Strategy::Hierarchical`, we'll end up the
        // "closest" `pyproject.toml` file for every Python file later on, so these act
        // as the "default" settings.)
        let settings = Settings::from_configuration(Configuration::default(), &path_dedot::CWD)?;
        Ok(Strategy::Hierarchical(settings))
    }
}

fn inner_main() -> Result<ExitCode> {
    // Extract command-line arguments.
    let (cli, overrides) = Cli::parse().partition();
    let log_level = extract_log_level(&cli);
    set_up_logging(&log_level)?;

    if cli.show_settings && cli.show_files {
        anyhow::bail!("specify --show-settings or show-files (not both)")
    }
    if let Some(shell) = cli.generate_shell_completion {
        shell.generate(&mut Cli::command(), &mut io::stdout());
        return Ok(ExitCode::SUCCESS);
    }

    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside of the hierarchy.
    let strategy = resolve(cli.config, &overrides)?;

    // Extract options that are included in `Settings`, but only apply at the top
    // level.
    let (fix, format) = match &strategy {
        Strategy::Fixed(settings) => (settings.fix, settings.format),
        Strategy::Hierarchical(settings) => (settings.fix, settings.format),
    };
    let autofix = if fix {
        fixer::Mode::Apply
    } else if matches!(format, SerializationFormat::Json) {
        fixer::Mode::Generate
    } else {
        fixer::Mode::None
    };

    if let Some(code) = cli.explain {
        commands::explain(&code, &format)?;
        return Ok(ExitCode::SUCCESS);
    }
    if cli.show_settings {
        commands::show_settings(&cli.files, &strategy, &overrides)?;
        return Ok(ExitCode::SUCCESS);
    }
    if cli.show_files {
        commands::show_files(&cli.files, &strategy, &overrides)?;
        return Ok(ExitCode::SUCCESS);
    }

    // Initialize the cache.
    let mut cache_enabled: bool = !cli.no_cache;
    if cache_enabled && cache::init().is_err() {
        eprintln!("Unable to initialize cache; disabling...");
        cache_enabled = false;
    }

    let printer = Printer::new(&format, &log_level);
    if cli.watch {
        if matches!(autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            eprintln!("Warning: --fix is not enabled in watch mode.");
        }
        if cli.add_noqa {
            eprintln!("Warning: --no-qa is not enabled in watch mode.");
        }
        if cli.autoformat {
            eprintln!("Warning: --autoformat is not enabled in watch mode.");
        }
        if format != SerializationFormat::Text {
            eprintln!("Warning: --format 'text' is used in watch mode.");
        }

        // Perform an initial run instantly.
        printer.clear_screen()?;
        printer.write_to_user("Starting linter in watch mode...\n");

        let messages = commands::run(
            &cli.files,
            &strategy,
            &overrides,
            cache_enabled,
            &fixer::Mode::None,
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
                        printer.clear_screen()?;
                        printer.write_to_user("File change detected...\n");

                        let messages = commands::run(
                            &cli.files,
                            &strategy,
                            &overrides,
                            cache_enabled,
                            &fixer::Mode::None,
                        )?;
                        printer.write_continuously(&messages)?;
                    }
                }
                Err(err) => return Err(err.into()),
            }
        }
    } else if cli.add_noqa {
        let modifications = commands::add_noqa(&cli.files, &strategy, &overrides)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Added {modifications} noqa directives.");
        }
    } else if cli.autoformat {
        let modifications = commands::autoformat(&cli.files, &strategy, &overrides)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Formatted {modifications} files.");
        }
    } else {
        let is_stdin = cli.files == vec![PathBuf::from("-")];

        // Generate lint violations.
        let diagnostics = if is_stdin {
            let filename = cli.stdin_filename.unwrap_or_else(|| "-".to_string());
            let path = Path::new(&filename);
            commands::run_stdin(&strategy, path, &autofix)?
        } else {
            commands::run(&cli.files, &strategy, &overrides, cache_enabled, &autofix)?
        };

        // Always try to print violations (the printer itself may suppress output),
        // unless we're writing fixes via stdin (in which case, the transformed
        // source code goes to stdout).
        if !(is_stdin && matches!(autofix, fixer::Mode::Apply)) {
            printer.write_once(&diagnostics, &autofix)?;
        }

        // Check for updates if we're in a non-silent log level.
        #[cfg(feature = "update-informer")]
        if !is_stdin && log_level >= LogLevel::Default && atty::is(atty::Stream::Stdout) {
            drop(updates::check_for_updates());
        }

        if !diagnostics.messages.is_empty() && !cli.exit_zero {
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{} {err:?}", "error".red().bold());
            ExitCode::FAILURE
        }
    }
}
