//! We want to ensure that once formatted content stays the same when formatted again, which is
//! known as formatter stability or formatter idempotency, and that the formatter prints
//! syntactically valid code. As our test cases cover only a limited amount of code, this allows
//! checking entire repositories.

use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Write;
use std::io::{stdout, BufWriter};
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use std::{fmt, fs, iter};

use anyhow::{bail, Context};
use clap::Parser;
use log::debug;
use similar::{ChangeTag, TextDiff};

use ruff::resolver::python_files_in_path;
use ruff::settings::types::{FilePattern, FilePatternSet};
use ruff_cli::args::CheckArgs;
use ruff_cli::resolve::resolve;
use ruff_python_formatter::{format_module, PyFormatOptions};

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
    /// Write all errors to this file in addition to stdout
    #[arg(long)]
    pub(crate) error_file: Option<PathBuf>,
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
        check_multi_project(args)
    } else {
        let result = check_repo(args)?;

        #[allow(clippy::print_stdout)]
        {
            print!("{}", result.display(args.format));
            println!(
                "Found {} stability errors in {} files in {:.2}s",
                result.diagnostics.len(),
                result.file_count,
                result.duration.as_secs_f32(),
            );
        }

        result.is_success()
    };
    if all_success {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

enum Message {
    Start {
        path: PathBuf,
    },
    Failed {
        path: PathBuf,
        error: anyhow::Error,
    },
    Finished {
        path: PathBuf,
        result: CheckRepoResult,
    },
}

fn check_multi_project(args: &Args) -> bool {
    let mut all_success = true;
    let mut total_errors = 0;
    let mut total_files = 0;
    let start = Instant::now();

    rayon::scope(|scope| {
        let (sender, receiver) = channel();

        for base_dir in &args.files {
            for dir in base_dir.read_dir().unwrap() {
                let path = dir.unwrap().path().clone();

                let sender = sender.clone();

                scope.spawn(move |_| {
                    sender.send(Message::Start { path: path.clone() }).unwrap();

                    match check_repo(&Args {
                        files: vec![path.clone()],
                        error_file: args.error_file.clone(),
                        ..*args
                    }) {
                        Ok(result) => sender.send(Message::Finished { result, path }),
                        Err(error) => sender.send(Message::Failed { error, path }),
                    }
                    .unwrap();
                });
            }
        }

        scope.spawn(|_| {
            let mut stdout = stdout().lock();
            let mut error_file = args.error_file.as_ref().map(|error_file| {
                BufWriter::new(File::create(error_file).expect("Couldn't open error file"))
            });

            for message in receiver {
                match message {
                    Message::Start { path } => {
                        writeln!(stdout, "Starting {}", path.display()).unwrap();
                    }
                    Message::Finished { path, result } => {
                        total_errors += result.diagnostics.len();
                        total_files += result.file_count;

                        writeln!(
                            stdout,
                            "Finished {} with {} files in {:.2}s",
                            path.display(),
                            result.file_count,
                            result.duration.as_secs_f32(),
                        )
                        .unwrap();
                        write!(stdout, "{}", result.display(args.format)).unwrap();
                        if let Some(error_file) = &mut error_file {
                            write!(error_file, "{}", result.display(args.format)).unwrap();
                        }
                        all_success = all_success && result.is_success();
                    }
                    Message::Failed { path, error } => {
                        writeln!(stdout, "Failed {}: {}", path.display(), error).unwrap();
                        all_success = false;
                    }
                }
            }
        });
    });

    let duration = start.elapsed();

    #[allow(clippy::print_stdout)]
    {
        println!(
            "{total_errors} stability errors in {total_files} files in {}s",
            duration.as_secs_f32()
        );
    }

    all_success
}

/// Returns whether the check was successful
fn check_repo(args: &Args) -> anyhow::Result<CheckRepoResult> {
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

    if paths.is_empty() {
        bail!("no python files in {:?}", cli.files)
    }

    let mut formatted_counter = 0;
    let errors: Vec<_> = paths
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
                    if let Some(message) = panic.downcast_ref::<String>() {
                        Err(FormatterStabilityError::Panic {
                            message: message.clone(),
                        })
                    } else if let Some(&message) = panic.downcast_ref::<&str>() {
                        Err(FormatterStabilityError::Panic {
                            message: message.to_string(),
                        })
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
            Err(error) => Some(Diagnostic { file, error }),
            Ok(()) => None,
        })
        .collect();

    let duration = start.elapsed();

    Ok(CheckRepoResult {
        duration,
        file_count: formatted_counter,
        diagnostics: errors,
    })
}

/// A compact diff that only shows a header and changes, but nothing unchanged. This makes viewing
/// multiple errors easier.
fn diff_show_only_changes(
    writer: &mut Formatter,
    formatted: &str,
    reformatted: &str,
) -> fmt::Result {
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
            writer.write_str(change.value())?;
        }
    }
    Ok(())
}

struct CheckRepoResult {
    duration: Duration,
    file_count: usize,
    diagnostics: Vec<Diagnostic>,
}

impl CheckRepoResult {
    fn display(&self, format: Format) -> DisplayCheckRepoResult {
        DisplayCheckRepoResult {
            result: self,
            format,
        }
    }

    fn is_success(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

struct DisplayCheckRepoResult<'a> {
    result: &'a CheckRepoResult,
    format: Format,
}

impl Display for DisplayCheckRepoResult<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for diagnostic in &self.result.diagnostics {
            write!(f, "{}", diagnostic.display(self.format))?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Diagnostic {
    file: PathBuf,
    error: FormatterStabilityError,
}

impl Diagnostic {
    fn display(&self, format: Format) -> DisplayDiagnostic {
        DisplayDiagnostic {
            diagnostic: self,
            format,
        }
    }
}

struct DisplayDiagnostic<'a> {
    format: Format,
    diagnostic: &'a Diagnostic,
}

impl Display for DisplayDiagnostic<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Diagnostic { file, error } = &self.diagnostic;

        match error {
            FormatterStabilityError::Unstable {
                formatted,
                reformatted,
            } => {
                writeln!(f, "Unstable formatting {}", file.display())?;
                match self.format {
                    Format::Minimal => {}
                    Format::Default => {
                        diff_show_only_changes(f, formatted, reformatted)?;
                    }
                    Format::Full => {
                        let diff = TextDiff::from_lines(formatted.as_str(), reformatted.as_str())
                            .unified_diff()
                            .header("Formatted once", "Formatted twice")
                            .to_string();
                        writeln!(
                            f,
                            r#"Reformatting the formatted code a second time resulted in formatting changes.
---
{diff}---

Formatted once:
---
{formatted}---

Formatted twice:
---
{reformatted}---\n"#,
                        )?;
                    }
                }
            }
            FormatterStabilityError::InvalidSyntax { err, formatted } => {
                writeln!(
                    f,
                    "Formatter generated invalid syntax {}: {}",
                    file.display(),
                    err
                )?;
                if self.format == Format::Full {
                    writeln!(f, "---\n{formatted}\n---\n")?;
                }
            }
            FormatterStabilityError::Panic { message } => {
                writeln!(f, "Panic {}: {}", file.display(), message)?;
            }
            FormatterStabilityError::Other(err) => {
                writeln!(f, "Uncategorized error {}: {}", file.display(), err)?;
            }
        }
        Ok(())
    }
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
    let printed = match format_module(&content, PyFormatOptions::default()) {
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

    let reformatted = match format_module(formatted, PyFormatOptions::default()) {
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
