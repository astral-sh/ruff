//! We want to ensure that once formatted content stays the same when formatted again, which is
//! known as formatter stability or formatter idempotency, and that the formatter prints
//! syntactically valid code. As our test cases cover only a limited amount of code, this allows
//! checking entire repositories.
#![allow(clippy::print_stdout)]

use anyhow::Context;
use clap::Parser;
use log::debug;
use ruff::resolver::python_files_in_path;
use ruff::settings::types::{FilePattern, FilePatternSet};
use ruff_cli::args::CheckArgs;
use ruff_cli::resolve::resolve;
use ruff_python_formatter::format_module;
use similar::{ChangeTag, TextDiff};
use std::io::Write;
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use std::{fs, io, iter};

/// Control the verbosity of the output
#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum, Default)]
pub(crate) enum Format {
    /// Filenames only
    Minimal,
    /// Filenames and reduced diff
    #[default]
    Default,
    /// Full diff and invalid code
    Full,
}

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Like `ruff check`'s files
    pub(crate) files: Vec<PathBuf>,
    /// Control the verbosity of the output
    #[arg(long, default_value_t, value_enum)]
    pub(crate) format: Format,
    /// Print only the first error and exit, `-x` is same as pytest
    #[arg(long, short = 'x')]
    pub(crate) exit_first_error: bool,
    /// Checks each project inside a directory
    #[arg(long)]
    pub(crate) multi_project: bool,
}

/// Generate ourself a `try_parse_from` impl for `CheckArgs`. This is a strange way to use clap but
/// we want the same behaviour as `ruff_cli` and clap seems to lack a way to parse directly to
/// `Args` instead of a `Parser`
#[derive(Debug, clap::Parser)]
struct WrapperArgs {
    #[clap(flatten)]
    check_args: CheckArgs,
}

pub(crate) fn main(args: &Args) -> anyhow::Result<ExitCode> {
    let all_success = if args.multi_project {
        let mut all_success = true;
        for dir in args.files[0].read_dir()? {
            let dir = dir?;
            println!("Starting {}", dir.path().display());
            let success = check_repo(&Args {
                files: vec![dir.path().to_path_buf()],
                format: args.format,
                exit_first_error: args.exit_first_error,
                multi_project: args.multi_project,
            });
            println!("Finished {}: {:?}", dir.path().display(), success);
            if !matches!(success, Ok(true)) {
                all_success = false;
            }
        }
        all_success
    } else {
        check_repo(args)?
    };
    if all_success {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

/// Returns whether the check was successful
pub(crate) fn check_repo(args: &Args) -> anyhow::Result<bool> {
    let start = Instant::now();

    // Find files to check (or in this case, format twice). Adapted from ruff_cli
    // First argument is ignored
    let dummy = PathBuf::from("check");
    let check_args_input = iter::once(&dummy).chain(&args.files);
    let check_args: CheckArgs = WrapperArgs::try_parse_from(check_args_input)?.check_args;
    let (cli, overrides) = check_args.partition();
    let mut pyproject_config = resolve(
        cli.isolated,
        cli.config.as_deref(),
        &overrides,
        cli.stdin_filename.as_deref(),
    )?;
    // We don't want to format pyproject.toml
    pyproject_config.settings.lib.include = FilePatternSet::try_from_vec(vec![
        FilePattern::Builtin("*.py"),
        FilePattern::Builtin("*.pyi"),
    ])
    .unwrap();
    let (paths, _resolver) = python_files_in_path(&cli.files, &pyproject_config, &overrides)?;
    assert!(!paths.is_empty(), "no python files in {:?}", cli.files);

    let mut formatted_counter = 0;
    let errors = paths
        .into_iter()
        .map(|dir_entry| {
            // Doesn't make sense to recover here in this test script
            dir_entry.expect("Iterating the files in the repository failed")
        })
        .filter(|dir_entry| {
            // For some reason it does not filter in the beginning
            dir_entry.file_name() != "pyproject.toml"
        })
        .map(|dir_entry| {
            let file = dir_entry.path().to_path_buf();
            formatted_counter += 1;
            // Handle panics (mostly in `debug_assert!`)
            let result = match catch_unwind(|| check_file(&file)) {
                Ok(result) => result,
                Err(panic) => {
                    if let Ok(message) = panic.downcast::<String>() {
                        Err(FormatterStabilityError::Panic { message: *message })
                    } else {
                        Err(FormatterStabilityError::Panic {
                            // This should not happen, but it can
                            message: "(Panic didn't set a string message)".to_string(),
                        })
                    }
                }
            };
            (result, file)
        })
        // We only care about the errors
        .filter_map(|(result, file)| match result {
            Err(err) => Some((err, file)),
            Ok(()) => None,
        });

    let mut any_errors = false;

    // Don't collect the iterator so we already see errors while it's still processing
    for (error, file) in errors {
        any_errors = true;
        match error {
            FormatterStabilityError::Unstable {
                formatted,
                reformatted,
            } => {
                println!("Unstable formatting {}", file.display());
                match args.format {
                    Format::Minimal => {}
                    Format::Default => {
                        diff_show_only_changes(
                            io::stdout().lock().by_ref(),
                            &formatted,
                            &reformatted,
                        )?;
                    }
                    Format::Full => {
                        let diff = TextDiff::from_lines(&formatted, &reformatted)
                            .unified_diff()
                            .header("Formatted once", "Formatted twice")
                            .to_string();
                        println!(
                            r#"Reformatting the formatted code a second time resulted in formatting changes.
---
{diff}---

Formatted once:
---
{formatted}---

Formatted twice:
---
{reformatted}---"#,
                        );
                    }
                }
            }
            FormatterStabilityError::InvalidSyntax { err, formatted } => {
                println!(
                    "Formatter generated invalid syntax {}: {}",
                    file.display(),
                    err
                );
                if args.format == Format::Full {
                    println!("---\n{formatted}\n---\n");
                }
            }
            FormatterStabilityError::Panic { message } => {
                println!("Panic {}: {}", file.display(), message);
            }
            FormatterStabilityError::Other(err) => {
                println!("Uncategorized error {}: {}", file.display(), err);
            }
        }

        if args.exit_first_error {
            return Ok(false);
        }
    }
    let duration = start.elapsed();
    println!(
        "Formatting {} files twice took {:.2}s",
        formatted_counter,
        duration.as_secs_f32()
    );

    if any_errors {
        Ok(false)
    } else {
        Ok(true)
    }
}

/// A compact diff that only shows a header and changes, but nothing unchanged. This makes viewing
/// multiple errors easier.
fn diff_show_only_changes(
    writer: &mut impl Write,
    formatted: &str,
    reformatted: &str,
) -> io::Result<()> {
    for changes in TextDiff::from_lines(formatted, reformatted)
        .unified_diff()
        .iter_hunks()
    {
        for (idx, change) in changes
            .iter_changes()
            .filter(|change| change.tag() != ChangeTag::Equal)
            .enumerate()
        {
            if idx == 0 {
                writeln!(writer, "{}", changes.header())?;
            }
            write!(writer, "{}", change.tag())?;
            writer.write_all(change.value().as_bytes())?;
        }
    }
    Ok(())
}

#[derive(Debug)]
enum FormatterStabilityError {
    /// First and second pass of the formatter are different
    Unstable {
        formatted: String,
        reformatted: String,
    },
    /// The formatter printed invalid code
    InvalidSyntax {
        err: anyhow::Error,
        formatted: String,
    },
    /// From `catch_unwind`
    Panic {
        message: String,
    },
    Other(anyhow::Error),
}

impl From<anyhow::Error> for FormatterStabilityError {
    fn from(error: anyhow::Error) -> Self {
        Self::Other(error)
    }
}

/// Run the formatter twice on the given file. Does not write back to the file
fn check_file(input_path: &Path) -> Result<(), FormatterStabilityError> {
    let content = fs::read_to_string(input_path).context("Failed to read file")?;
    let printed = match format_module(&content) {
        Ok(printed) => printed,
        Err(err) => {
            return if err
                .to_string()
                .starts_with("Source contains syntax errors ")
            {
                debug!(
                    "Skipping {} with invalid first pass {}",
                    input_path.display(),
                    err
                );
                Ok(())
            } else {
                Err(err.into())
            };
        }
    };
    let formatted = printed.as_code();

    let reformatted = match format_module(formatted) {
        Ok(reformatted) => reformatted,
        Err(err) => {
            return Err(FormatterStabilityError::InvalidSyntax {
                err,
                formatted: formatted.to_string(),
            });
        }
    };

    if reformatted.as_code() != formatted {
        return Err(FormatterStabilityError::Unstable {
            formatted: formatted.to_string(),
            reformatted: reformatted.into_code(),
        });
    }
    Ok(())
}
