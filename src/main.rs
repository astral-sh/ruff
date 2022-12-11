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

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::Instant;

use ::ruff::autofix::fixer;
use ::ruff::checks::{CheckCode, CheckKind};
use ::ruff::cli::{extract_log_level, Cli, Overrides};
use ::ruff::fs::collect_python_files;
use ::ruff::iterators::par_iter;
use ::ruff::linter::{add_noqa_to_path, autoformat_path, lint_path, lint_stdin, Diagnostics};
use ::ruff::logging::{set_up_logging, LogLevel};
use ::ruff::message::Message;
use ::ruff::printer::Printer;
use ::ruff::settings::configuration::Configuration;
use ::ruff::settings::types::SerializationFormat;
use ::ruff::settings::{pyproject, Settings};
#[cfg(feature = "update-informer")]
use ::ruff::updates;
use ::ruff::{cache, commands};
use anyhow::Result;
use clap::{CommandFactory, Parser};
use colored::Colorize;
use log::{debug, error};
use notify::{recommended_watcher, RecursiveMode, Watcher};
use path_absolutize::path_dedot;
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
use rustpython_ast::Location;

fn read_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn run_once_stdin(
    settings: &Settings,
    filename: &Path,
    autofix: &fixer::Mode,
) -> Result<Diagnostics> {
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(filename, &stdin, settings, autofix)?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}

fn run_once(
    files: &[PathBuf],
    defaults: &Settings,
    overrides: &Overrides,
    cache: bool,
    autofix: &fixer::Mode,
) -> Diagnostics {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) = collect_python_files(files, overrides, defaults);
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let mut diagnostics: Diagnostics = par_iter(&paths)
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let settings = resolver.resolve(path).unwrap_or(defaults);
                    lint_path(path, settings, &cache.into(), autofix)
                        .map_err(|e| (Some(path.to_owned()), e.to_string()))
                }
                Err(e) => Err((
                    e.path().map(Path::to_owned),
                    e.io_error()
                        .map_or_else(|| e.to_string(), io::Error::to_string),
                )),
            }
            .unwrap_or_else(|(path, message)| {
                if let Some(path) = &path {
                    let settings = resolver.resolve(path).unwrap_or(defaults);
                    if settings.enabled.contains(&CheckCode::E902) {
                        Diagnostics::new(vec![Message {
                            kind: CheckKind::IOError(message),
                            location: Location::default(),
                            end_location: Location::default(),
                            fix: None,
                            filename: path.to_string_lossy().to_string(),
                            source: None,
                        }])
                    } else {
                        error!("Failed to check {}: {message}", path.to_string_lossy());
                        Diagnostics::default()
                    }
                } else {
                    error!("{message}");
                    Diagnostics::default()
                }
            })
        })
        .reduce(Diagnostics::default, |mut acc, item| {
            acc += item;
            acc
        });

    diagnostics.messages.sort_unstable();
    let duration = start.elapsed();
    debug!("Checked files in: {:?}", duration);

    diagnostics
}

fn add_noqa(files: &[PathBuf], defaults: &Settings, overrides: &Overrides) -> usize {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) = collect_python_files(files, overrides, defaults);
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications: usize = par_iter(&paths)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let settings = resolver.resolve(path).unwrap_or(defaults);
            match add_noqa_to_path(path, settings) {
                Ok(count) => Some(count),
                Err(e) => {
                    error!("Failed to add noqa to {}: {e}", path.to_string_lossy());
                    None
                }
            }
        })
        .sum();

    let duration = start.elapsed();
    debug!("Added noqa to files in: {:?}", duration);

    modifications
}

fn autoformat(files: &[PathBuf], defaults: &Settings, overrides: &Overrides) -> usize {
    // Collect all the files to format.
    let start = Instant::now();
    let (paths, resolver) = collect_python_files(files, overrides, defaults);
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications = par_iter(&paths)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let settings = resolver.resolve(path).unwrap_or(defaults);
            match autoformat_path(path, settings) {
                Ok(()) => Some(()),
                Err(e) => {
                    error!("Failed to autoformat {}: {e}", path.to_string_lossy());
                    None
                }
            }
        })
        .count();

    let duration = start.elapsed();
    debug!("Auto-formatted files in: {:?}", duration);

    modifications
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

    // Find the `pyproject.toml`.
    let pyproject = cli
        .config
        .or_else(|| pyproject::find_pyproject_toml(&path_dedot::CWD));

    // Reconcile configuration from `pyproject.toml` and command-line arguments.
    let mut configuration = pyproject
        .as_ref()
        .map(|path| Configuration::from_pyproject(path))
        .transpose()?
        .unwrap_or_default();
    configuration.merge(overrides.clone());

    if cli.show_settings {
        // TODO(charlie): This would be more useful if required a single file, and told
        // you the settings used to lint that file.
        commands::show_settings(&configuration, pyproject.as_deref());
        return Ok(ExitCode::SUCCESS);
    }

    // Extract options that are included in the `pyproject.toml`, but aren't in
    // `Settings`.
    let fix = if configuration.fix {
        fixer::Mode::Apply
    } else if matches!(configuration.format, SerializationFormat::Json) {
        fixer::Mode::Generate
    } else {
        fixer::Mode::None
    };
    let format = configuration.format;

    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside of the hierarchy.
    let defaults = Settings::from_configuration(
        configuration,
        pyproject.as_ref().and_then(|path| path.parent()),
    )?;

    if let Some(code) = cli.explain {
        commands::explain(&code, format)?;
        return Ok(ExitCode::SUCCESS);
    }

    if cli.show_files {
        commands::show_files(&cli.files, &defaults, &overrides);
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
        if matches!(fix, fixer::Mode::Generate | fixer::Mode::Apply) {
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

        let messages = run_once(
            &cli.files,
            &defaults,
            &overrides,
            cache_enabled,
            &fixer::Mode::None,
        );
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
                    let py_changed = paths.iter().any(|p| {
                        p.extension()
                            .map(|ext| ext == "py" || ext == "pyi")
                            .unwrap_or_default()
                    });
                    if py_changed {
                        printer.clear_screen()?;
                        printer.write_to_user("File change detected...\n");

                        let messages = run_once(
                            &cli.files,
                            &defaults,
                            &overrides,
                            cache_enabled,
                            &fixer::Mode::None,
                        );
                        printer.write_continuously(&messages)?;
                    }
                }
                Err(err) => return Err(err.into()),
            }
        }
    } else if cli.add_noqa {
        let modifications = add_noqa(&cli.files, &defaults, &overrides);
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Added {modifications} noqa directives.");
        }
    } else if cli.autoformat {
        let modifications = autoformat(&cli.files, &defaults, &overrides);
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Formatted {modifications} files.");
        }
    } else {
        let is_stdin = cli.files == vec![PathBuf::from("-")];

        // Generate lint violations.
        let diagnostics = if is_stdin {
            let filename = cli.stdin_filename.unwrap_or_else(|| "-".to_string());
            let path = Path::new(&filename);
            run_once_stdin(&defaults, path, &fix)?
        } else {
            run_once(&cli.files, &defaults, &overrides, cache_enabled, &fix)
        };

        // Always try to print violations (the printer itself may suppress output),
        // unless we're writing fixes via stdin (in which case, the transformed
        // source code goes to stdout).
        if !(is_stdin && matches!(fix, fixer::Mode::Apply)) {
            printer.write_once(&diagnostics, &fix)?;
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
