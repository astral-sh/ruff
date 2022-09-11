use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueHint};
use colored::Colorize;
use glob::Pattern;
use log::{debug, error};
use notify::{raw_watcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use walkdir::DirEntry;

use ::ruff::checks::CheckCode;
use ::ruff::checks::CheckKind;
use ::ruff::fs::iter_python_files;
use ::ruff::linter::lint_path;
use ::ruff::logging::set_up_logging;
use ::ruff::message::Message;
use ::ruff::settings::Settings;
use ::ruff::tell_user;

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[clap(name = format!("{CARGO_PKG_NAME} (v{CARGO_PKG_VERSION})"))]
#[clap(about = "An extremely fast Python linter.", long_about = None)]
struct Cli {
    #[clap(parse(from_os_str), value_hint = ValueHint::AnyPath, required = true)]
    files: Vec<PathBuf>,
    /// Enable verbose logging.
    #[clap(short, long, action)]
    verbose: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting errors).
    #[clap(short, long, action)]
    quiet: bool,
    /// Exit with status code "0", even upon detecting errors.
    #[clap(short, long, action)]
    exit_zero: bool,
    /// Run in watch mode by re-running whenever files change.
    #[clap(short, long, action)]
    watch: bool,
    /// Attempt to automatically fix lint errors.
    #[clap(short, long, action)]
    fix: bool,
    /// Disable cache reads.
    #[clap(short, long, action)]
    no_cache: bool,
    /// List of error codes to enable.
    #[clap(long, multiple = true)]
    select: Vec<CheckCode>,
    /// List of error codes to ignore.
    #[clap(long, multiple = true)]
    ignore: Vec<CheckCode>,
    /// List of file and/or directory patterns to exclude from checks.
    #[clap(long, multiple = true)]
    exclude: Vec<Pattern>,
}

#[cfg(feature = "update-informer")]
fn check_for_updates() {
    use update_informer::{registry, Check};

    let informer = update_informer::new(registry::PyPI, CARGO_PKG_NAME, CARGO_PKG_VERSION);

    if let Some(new_version) = informer.check_version().ok().flatten() {
        let msg = format!(
            "A new version of {pkg_name} is available: v{pkg_version} -> {new_version}",
            pkg_name = CARGO_PKG_NAME.italic().cyan(),
            pkg_version = CARGO_PKG_VERSION,
            new_version = new_version.to_string().green()
        );

        let cmd = format!(
            "Run to update: {cmd} {pkg_name}",
            cmd = "pip3 install --upgrade".green(),
            pkg_name = CARGO_PKG_NAME.green()
        );

        println!("\n{msg}\n{cmd}");
    }
}

fn run_once(
    files: &[PathBuf],
    settings: &Settings,
    cache: bool,
    autofix: bool,
) -> Result<Vec<Message>> {
    // Collect all the files to check.
    let start = Instant::now();
    let paths: Vec<DirEntry> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude))
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let mut messages: Vec<Message> = paths
        .par_iter()
        .map(|entry| {
            lint_path(entry.path(), settings, &cache.into(), &autofix.into()).unwrap_or_else(|e| {
                error!("Failed to check {}: {e:?}", entry.path().to_string_lossy());
                vec![]
            })
        })
        .flatten()
        .collect();

    if settings.select.contains(&CheckCode::E902) {
        for file in files {
            if !file.exists() {
                messages.push(Message {
                    kind: CheckKind::IOError(file.to_string_lossy().to_string()),
                    fixed: false,
                    location: Default::default(),
                    filename: file.to_string_lossy().to_string(),
                })
            }
        }
    }

    messages.sort_unstable();
    let duration = start.elapsed();
    debug!("Checked files in: {:?}", duration);

    Ok(messages)
}

fn report_once(messages: &[Message]) -> Result<()> {
    let (fixed, outstanding): (Vec<&Message>, Vec<&Message>) =
        messages.iter().partition(|message| message.fixed);
    let num_fixable = outstanding
        .iter()
        .filter(|message| message.kind.fixable())
        .count();

    if !outstanding.is_empty() {
        for message in &outstanding {
            println!("{}", message);
        }
        println!();
    }

    if !fixed.is_empty() {
        println!(
            "Found {} error(s) ({} fixed).",
            outstanding.len(),
            fixed.len()
        );
    } else {
        println!("Found {} error(s).", outstanding.len());
    }

    if num_fixable > 0 {
        println!("{num_fixable} potentially fixable with the --fix option.");
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
    if !cli.exclude.is_empty() {
        settings.exclude(cli.exclude);
    }

    if cli.watch {
        if cli.fix {
            println!("Warning: --fix is not enabled in watch mode.")
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
        let messages = run_once(&cli.files, &settings, !cli.no_cache, cli.fix)?;
        if !cli.quiet {
            report_once(&messages)?;
        }

        #[cfg(feature = "update-informer")]
        check_for_updates();

        if !messages.is_empty() && !cli.exit_zero {
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(_) => ExitCode::FAILURE,
    }
}
