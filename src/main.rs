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
use ::ruff::cli::{extract_log_level, Cli, Overrides};
use ::ruff::logging::{set_up_logging, LogLevel};
use ::ruff::printer::Printer;
use ::ruff::resolver::{resolve_settings, FileDiscovery, PyprojectDiscovery, Relativity};
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

/// Resolve the relevant settings strategy and defaults for the current
/// invocation.
fn resolve(
    config: Option<&Path>,
    overrides: &Overrides,
    stdin_filename: Option<&Path>,
) -> Result<PyprojectDiscovery> {
    if let Some(pyproject) = config {
        // First priority: the user specified a `pyproject.toml` file. Use that
        // `pyproject.toml` for _all_ configuration, and resolve paths relative to the
        // current working directory. (This matches ESLint's behavior.)
        let settings = resolve_settings(pyproject, &Relativity::Cwd, Some(overrides))?;
        Ok(PyprojectDiscovery::Fixed(settings))
    } else if let Some(pyproject) = pyproject::find_pyproject_toml(
        stdin_filename
            .as_ref()
            .unwrap_or(&path_dedot::CWD.as_path()),
    )? {
        // Second priority: find a `pyproject.toml` file in either an ancestor of
        // `stdin_filename` (if set) or the current working path all paths relative to
        // that directory. (With `Strategy::Hierarchical`, we'll end up finding
        // the "closest" `pyproject.toml` file for every Python file later on,
        // so these act as the "default" settings.)
        let settings = resolve_settings(&pyproject, &Relativity::Parent, Some(overrides))?;
        Ok(PyprojectDiscovery::Hierarchical(settings))
    } else if let Some(pyproject) = pyproject::find_user_pyproject_toml() {
        // Third priority: find a user-specific `pyproject.toml`, but resolve all paths
        // relative the current working directory. (With `Strategy::Hierarchical`, we'll
        // end up the "closest" `pyproject.toml` file for every Python file later on, so
        // these act as the "default" settings.)
        let settings = resolve_settings(&pyproject, &Relativity::Cwd, Some(overrides))?;
        Ok(PyprojectDiscovery::Hierarchical(settings))
    } else {
        // Fallback: load Ruff's default settings, and resolve all paths relative to the
        // current working directory. (With `Strategy::Hierarchical`, we'll end up the
        // "closest" `pyproject.toml` file for every Python file later on, so these act
        // as the "default" settings.)
        let mut config = Configuration::default();
        // Apply command-line options that override defaults.
        config.apply(overrides.clone());
        let settings = Settings::from_configuration(config, &path_dedot::CWD)?;
        Ok(PyprojectDiscovery::Hierarchical(settings))
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
    let pyproject_strategy = resolve(
        cli.config.as_deref(),
        &overrides,
        cli.stdin_filename.as_deref(),
    )?;

    // Extract options that are included in `Settings`, but only apply at the top
    // level.
    let file_strategy = FileDiscovery {
        force_exclude: match &pyproject_strategy {
            PyprojectDiscovery::Fixed(settings) => settings.force_exclude,
            PyprojectDiscovery::Hierarchical(settings) => settings.force_exclude,
        },
        respect_gitignore: match &pyproject_strategy {
            PyprojectDiscovery::Fixed(settings) => settings.respect_gitignore,
            PyprojectDiscovery::Hierarchical(settings) => settings.respect_gitignore,
        },
    };
    let (fix, format) = match &pyproject_strategy {
        PyprojectDiscovery::Fixed(settings) => (settings.fix, settings.format),
        PyprojectDiscovery::Hierarchical(settings) => (settings.fix, settings.format),
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
        commands::show_settings(&cli.files, &pyproject_strategy, &file_strategy, &overrides)?;
        return Ok(ExitCode::SUCCESS);
    }
    if cli.show_files {
        commands::show_files(&cli.files, &pyproject_strategy, &file_strategy, &overrides)?;
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
            &pyproject_strategy,
            &file_strategy,
            &overrides,
            cache_enabled.into(),
            fixer::Mode::None,
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
                            &pyproject_strategy,
                            &file_strategy,
                            &overrides,
                            cache_enabled.into(),
                            fixer::Mode::None,
                        )?;
                        printer.write_continuously(&messages)?;
                    }
                }
                Err(err) => return Err(err.into()),
            }
        }
    } else if cli.add_noqa {
        let modifications =
            commands::add_noqa(&cli.files, &pyproject_strategy, &file_strategy, &overrides)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Added {modifications} noqa directives.");
        }
    } else if cli.autoformat {
        let modifications =
            commands::autoformat(&cli.files, &pyproject_strategy, &file_strategy, &overrides)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Formatted {modifications} files.");
        }
    } else {
        let is_stdin = cli.files == vec![PathBuf::from("-")];

        // Generate lint violations.
        let diagnostics = if is_stdin {
            commands::run_stdin(cli.stdin_filename.as_deref(), &pyproject_strategy, autofix)?
        } else {
            commands::run(
                &cli.files,
                &pyproject_strategy,
                &file_strategy,
                &overrides,
                cache_enabled.into(),
                autofix,
            )?
        };

        // Always try to print violations (the printer itself may suppress output),
        // unless we're writing fixes via stdin (in which case, the transformed
        // source code goes to stdout).
        if !(is_stdin && matches!(autofix, fixer::Mode::Apply)) {
            printer.write_once(&diagnostics, autofix)?;
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
