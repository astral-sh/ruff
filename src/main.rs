use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueHint};
use colored::Colorize;
use log::{debug, error};
use notify::{raw_watcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use walkdir::DirEntry;

use ::ruff::checks::CheckCode;
use ::ruff::fs::iter_python_files;
use ::ruff::linter::check_path;
use ::ruff::logging::set_up_logging;
use ::ruff::message::Message;
use ::ruff::settings::Settings;
use ::ruff::tell_user;

#[derive(Debug, Parser)]
#[clap(name = "ruff")]
#[clap(about = "An extremely fast Python linter.", long_about = None)]
struct Cli {
    #[clap(parse(from_os_str), value_hint = ValueHint::AnyPath, required = true)]
    files: Vec<PathBuf>,
    /// Enable verbose logging.
    #[clap(short, long, action)]
    verbose: bool,
    /// Enable autofix.
    #[clap(short, long, action)]
    autofix: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting errors).
    #[clap(short, long, action)]
    quiet: bool,
    /// Exit with status code "0", even upon detecting errors.
    #[clap(short, long, action)]
    exit_zero: bool,
    /// Run in watch mode by re-running whenever files change.
    #[clap(short, long, action)]
    watch: bool,
    /// Disable cache reads.
    #[clap(short, long, action)]
    no_cache: bool,
    /// Comma-separated list of error codes to enable.
    #[clap(long, multiple = true)]
    select: Vec<CheckCode>,
    /// Comma-separated list of error codes to ignore.
    #[clap(long, multiple = true)]
    ignore: Vec<CheckCode>,
}

fn run_once(
    files: &[PathBuf],
    settings: &Settings,
    cache: bool,
    autofix: bool,
) -> Result<Vec<Message>> {
    // Collect all the files to check.
    let start = Instant::now();
    let files: Vec<DirEntry> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude))
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let mut messages: Vec<Message> = files
        .par_iter()
        .map(|entry| {
            check_path(entry.path(), settings, &cache.into(), &autofix.into()).unwrap_or_else(|e| {
                error!("Failed to check {}: {e:?}", entry.path().to_string_lossy());
                vec![]
            })
        })
        .flatten()
        .collect();
    messages.sort_unstable();
    let duration = start.elapsed();
    debug!("Checked files in: {:?}", duration);

    Ok(messages)
}

fn report_once(messages: &[Message]) -> Result<()> {
    let (fixed, outstanding): (Vec<&Message>, Vec<&Message>) =
        messages.iter().partition(|message| {
            message
                .fix
                .as_ref()
                .map(|fix| fix.applied)
                .unwrap_or_default()
        });

    // TODO(charlie): If autofix is disabled, but some rules are fixable, tell the user.
    if fixed.is_empty() {
        println!("Found {} error(s).", messages.len());
    } else {
        println!("Found {} error(s) (fixed {}).", messages.len(), fixed.len());
    }

    if !outstanding.is_empty() {
        println!();
        for message in outstanding {
            println!("{}", message);
        }
    }

    Ok(())
}

fn report_continuously(messages: &[Message]) -> Result<()> {
    tell_user!(
        "Found {} error(s). Watching for file changes.",
        messages.len(),
    );

    if !messages.is_empty() {
        println!();
        for message in messages {
            println!("{}", message);
        }
    }

    Ok(())
}

fn inner_main() -> Result<ExitCode> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    // TODO(charlie): Can we avoid this cast?
    let paths: Vec<&Path> = cli.files.iter().map(PathBuf::as_path).collect();
    let mut settings = Settings::from_paths(paths)?;
    if !cli.select.is_empty() {
        settings.select(cli.select);
    }
    if !cli.ignore.is_empty() {
        settings.ignore(&cli.ignore);
    }

    if cli.watch {
        if cli.autofix {
            println!("Warning: autofix is not enabled in watch mode.")
        }

        // Perform an initial run instantly.
        clearscreen::clear()?;
        tell_user!("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files, &settings, !cli.no_cache, false)?;
        if !cli.quiet {
            report_continuously(&messages)?;
        }

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = raw_watcher(tx)?;
        for file in &cli.files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        loop {
            match rx.recv() {
                Ok(e) => {
                    if let Some(path) = e.path {
                        if path.to_string_lossy().ends_with(".py") {
                            clearscreen::clear()?;
                            tell_user!("File change detected...\n");

                            let messages = run_once(&cli.files, &settings, !cli.no_cache, false)?;
                            if !cli.quiet {
                                report_continuously(&messages)?;
                            }
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        let messages = run_once(&cli.files, &settings, !cli.no_cache, cli.autofix)?;
        if !cli.quiet {
            report_once(&messages)?;
        }

        check_for_updates();

        if !messages.is_empty() && !cli.exit_zero {
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn check_for_updates() {
    use update_informer::{registry, Check};

    let pkg_name = env!("CARGO_PKG_NAME");
    let pkg_version = env!("CARGO_PKG_VERSION");
    let informer = update_informer::new(registry::PyPI, pkg_name, pkg_version);

    if let Some(new_version) = informer.check_version().ok().flatten() {
        let msg = format!(
            "A new version of {pkg_name} is available: v{pkg_version} -> {new_version}",
            pkg_name = pkg_name.italic().cyan(),
            new_version = new_version.to_string().green()
        );

        let cmd = format!(
            "Run to update: {cmd} {pkg_name}",
            cmd = "pip3 install --upgrade".green(),
            pkg_name = pkg_name.green()
        );

        println!("\n{msg}\n{cmd}");
    }
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(_) => ExitCode::FAILURE,
    }
}
