use anyhow::{bail, Context};
use clap::{CommandFactory, FromArgMatches};
use ignore::DirEntry;
use indicatif::ProgressBar;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use ruff::resolver::python_files_in_path;
use ruff::settings::types::{FilePattern, FilePatternSet};
use ruff_cli::args::CheckArgs;
use ruff_cli::resolve::resolve;
use ruff_formatter::{FormatError, PrintError};
use ruff_python_formatter::{format_module, FormatModuleError, PyFormatOptions};
use similar::{ChangeTag, TextDiff};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::{Add, AddAssign};
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use std::{fmt, fs, io};
use tempfile::NamedTempFile;

/// Find files that ruff would check so we can format them. Adapted from `ruff_cli`.
fn ruff_check_paths(dirs: &[PathBuf]) -> anyhow::Result<Vec<Result<DirEntry, ignore::Error>>> {
    let args_matches = CheckArgs::command()
        .no_binary_name(true)
        .get_matches_from(dirs);
    let check_args: CheckArgs = CheckArgs::from_arg_matches(&args_matches)?;
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
        bail!("no python files in {:?}", dirs)
    }
    Ok(paths)
}

/// Currently only used for computing the Jaccard index
///
/// The [Jaccard index](https://en.wikipedia.org/wiki/Jaccard_index) can be defined as
/// ```
/// J(A, B) = |A∩B| / (|A\B| + |B\A| + |A∩B|)
/// ```
/// where in our case `A` is the black formatted input, `B` is the ruff formatted output and the
/// intersection are the lines in the diff that didn't change. We don't just track intersection and
/// union so we can make statistics about size changes.
#[derive(Default, Debug, Copy, Clone)]
pub(crate) struct Statistics {
    /// The size of `A\B`, the number of lines only in black formatted input
    black_input: u32,
    /// The size of `B\A`, the number of lines only in formatted output
    ruff_output: u32,
    /// The number of lines unchanged between during formatting
    intersection: u32,
}

impl Statistics {
    pub(crate) fn from_versions(black: &str, ruff: &str) -> Self {
        if black == ruff {
            let intersection = u32::try_from(black.lines().count()).unwrap();
            Self {
                black_input: 0,
                ruff_output: 0,
                intersection,
            }
        } else {
            let diff = TextDiff::from_lines(black, ruff);
            let mut statistics = Self::default();
            for change in diff.iter_all_changes() {
                match change.tag() {
                    ChangeTag::Delete => statistics.black_input += 1,
                    ChangeTag::Insert => statistics.ruff_output += 1,
                    ChangeTag::Equal => statistics.intersection += 1,
                }
            }
            statistics
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn jaccard_index(&self) -> f32 {
        self.intersection as f32 / (self.black_input + self.ruff_output + self.intersection) as f32
    }
}

impl Add<Statistics> for Statistics {
    type Output = Statistics;

    fn add(self, rhs: Statistics) -> Self::Output {
        Statistics {
            black_input: self.black_input + rhs.black_input,
            ruff_output: self.ruff_output + rhs.ruff_output,
            intersection: self.intersection + rhs.intersection,
        }
    }
}

impl AddAssign<Statistics> for Statistics {
    fn add_assign(&mut self, rhs: Statistics) {
        *self = *self + rhs;
    }
}

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

#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Like `ruff check`'s files
    pub(crate) files: Vec<PathBuf>,
    /// Check stability
    ///
    /// We want to ensure that once formatted content stays the same when formatted again, which is
    /// known as formatter stability or formatter idempotency, and that the formatter prints
    /// syntactically valid code. As our test cases cover only a limited amount of code, this allows
    /// checking entire repositories.
    #[arg(long)]
    pub(crate) stability_check: bool,
    /// Format the files. Without this flag, the input files are not modified
    #[arg(long)]
    pub(crate) write: bool,
    /// Control the verbosity of the output
    #[arg(long, default_value_t, value_enum)]
    pub(crate) format: Format,
    /// Print only the first error and exit, `-x` is same as pytest
    #[arg(long, short = 'x')]
    pub(crate) exit_first_error: bool,
    /// Checks each project inside a directory
    #[arg(long)]
    pub(crate) multi_project: bool,
    /// Write all errors to this file in addition to stdout. Only used in multi-project mode.
    #[arg(long)]
    pub(crate) error_file: Option<PathBuf>,
}

pub(crate) fn main(args: &Args) -> anyhow::Result<ExitCode> {
    let all_success = if args.multi_project {
        check_multi_project(args)
    } else {
        let result = check_repo(args, args.stability_check, args.write, true)?;

        #[allow(clippy::print_stdout)]
        {
            print!("{}", result.display(args.format));
            println!(
                "Found {} stability errors in {} files (jaccard index {:.3}) in {:.2}s",
                result
                    .diagnostics
                    .iter()
                    .filter(|diagnostics| !diagnostics.error.is_success())
                    .count(),
                result.file_count,
                result.statistics.jaccard_index(),
                result.duration.as_secs_f32(),
            );
        }

        !result.is_success()
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

                    let args = Args {
                        files: vec![path.clone()],
                        error_file: args.error_file.clone(),
                        ..*args
                    };
                    match check_repo(&args, args.stability_check, args.write, false) {
                        Ok(result) => sender.send(Message::Finished { result, path }),
                        Err(error) => sender.send(Message::Failed { error, path }),
                    }
                    .unwrap();
                });
            }
        }

        scope.spawn(|_| {
            let mut error_file = args.error_file.as_ref().map(|error_file| {
                BufWriter::new(File::create(error_file).expect("Couldn't open error file"))
            });

            let bar = ProgressBar::new(args.files.len() as u64);
            for message in receiver {
                match message {
                    Message::Start { path } => {
                        bar.println(path.display().to_string());
                    }
                    Message::Finished { path, result } => {
                        total_errors += result.diagnostics.len();
                        total_files += result.file_count;

                        bar.println(format!(
                            "Finished {} with {} files (jaccard index {:.3}) in {:.2}s",
                            path.display(),
                            result.file_count,
                            result.statistics.jaccard_index(),
                            result.duration.as_secs_f32(),
                        ));
                        bar.println(result.display(args.format).to_string().trim_end());
                        if let Some(error_file) = &mut error_file {
                            write!(error_file, "{}", result.display(args.format)).unwrap();
                        }
                        all_success = all_success && !result.is_success();
                        bar.inc(1);
                    }
                    Message::Failed { path, error } => {
                        bar.println(format!("Failed {}: {}", path.display(), error));
                        all_success = false;
                        bar.inc(1);
                    }
                }
            }
            bar.finish();
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
fn check_repo(
    args: &Args,
    stability_check: bool,
    write: bool,
    progress_bar: bool,
) -> anyhow::Result<CheckRepoResult> {
    let start = Instant::now();

    // Find files to check (or in this case, format twice). Adapted from ruff_cli
    // First argument is ignored
    let paths = ruff_check_paths(&args.files)?;

    let bar = progress_bar.then(|| ProgressBar::new(paths.len() as u64));
    let result_iter = paths
        .into_par_iter()
        .map(|dir_entry| {
            let dir_entry = match dir_entry.context("Iterating the files in the repository failed")
            {
                Ok(dir_entry) => dir_entry,
                Err(err) => return Err(err),
            };
            let file = dir_entry.path().to_path_buf();
            // For some reason it does not filter in the beginning
            if dir_entry.file_name() == "pyproject.toml" {
                return Ok((Ok(Statistics::default()), file));
            }

            let file = dir_entry.path().to_path_buf();
            // Handle panics (mostly in `debug_assert!`)
            let result = match catch_unwind(|| check_file(&file, stability_check, write)) {
                Ok(result) => result,
                Err(panic) => {
                    if let Some(message) = panic.downcast_ref::<String>() {
                        Err(CheckFileError::Panic {
                            message: message.clone(),
                        })
                    } else if let Some(&message) = panic.downcast_ref::<&str>() {
                        Err(CheckFileError::Panic {
                            message: message.to_string(),
                        })
                    } else {
                        Err(CheckFileError::Panic {
                            // This should not happen, but it can
                            message: "(Panic didn't set a string message)".to_string(),
                        })
                    }
                }
            };
            if let Some(bar) = &bar {
                bar.inc(1);
            }
            Ok((result, file))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if let Some(bar) = bar {
        bar.finish();
    }

    let mut statistics = Statistics::default();
    let mut formatted_counter = 0;
    let mut diagnostics = Vec::new();
    for (result, file) in result_iter {
        formatted_counter += 1;
        match result {
            Ok(statistics_file) => statistics += statistics_file,
            Err(error) => diagnostics.push(Diagnostic { file, error }),
        }
    }

    let duration = start.elapsed();

    Ok(CheckRepoResult {
        duration,
        file_count: formatted_counter,
        diagnostics,
        statistics,
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
    statistics: Statistics,
}

impl CheckRepoResult {
    fn display(&self, format: Format) -> DisplayCheckRepoResult {
        DisplayCheckRepoResult {
            result: self,
            format,
        }
    }

    /// Whether there was any ruff bug, as opposed to input files that were already broken or io
    /// errors
    fn is_success(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.error.is_success())
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
    error: CheckFileError,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Diagnostic { file, error } = &self.diagnostic;

        match error {
            CheckFileError::Unstable {
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
            CheckFileError::Panic { message } => {
                writeln!(f, "Panic {}: {}", file.display(), message)?;
            }
            CheckFileError::SyntaxErrorInInput(error) => {
                writeln!(f, "Syntax error in {}: {}", file.display(), error)?;
            }
            CheckFileError::SyntaxErrorInOutput { formatted, error } => {
                writeln!(
                    f,
                    "Formatter generated invalid syntax {}: {}",
                    file.display(),
                    error
                )?;
                if self.format == Format::Full {
                    writeln!(f, "---\n{formatted}\n---\n")?;
                }
            }
            CheckFileError::FormatError(error) => {
                writeln!(f, "Formatter error for {}: {}", file.display(), error)?;
            }
            CheckFileError::PrintError(error) => {
                writeln!(f, "Printer error for {}: {}", file.display(), error)?;
            }
            CheckFileError::IoError(error) => {
                writeln!(f, "Error reading {}: {}", file.display(), error)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
enum CheckFileError {
    /// First and second pass of the formatter are different
    Unstable {
        formatted: String,
        reformatted: String,
    },
    /// The input file was already invalid
    SyntaxErrorInInput(FormatModuleError),
    /// The formatter introduced a syntax error
    SyntaxErrorInOutput {
        formatted: String,
        error: FormatModuleError,
    },
    /// The formatter failed (bug)
    FormatError(FormatError),
    /// The printer failed (bug)
    PrintError(PrintError),
    /// Failed to read the file (this sometimes happens e.g. with strange filenames)
    IoError(io::Error),
    /// From `catch_unwind`
    Panic { message: String },
}

impl CheckFileError {
    /// Returns `false` if this is a formatter bug or `true` is if it is something outside of ruff
    fn is_success(&self) -> bool {
        match self {
            CheckFileError::Unstable { .. } => false,
            CheckFileError::SyntaxErrorInInput(_) => true,
            CheckFileError::SyntaxErrorInOutput { .. } => false,
            CheckFileError::FormatError(_) => false,
            CheckFileError::PrintError(_) => false,
            CheckFileError::IoError(_) => true,
            CheckFileError::Panic { .. } => false,
        }
    }
}

impl From<io::Error> for CheckFileError {
    fn from(value: io::Error) -> Self {
        Self::IoError(value)
    }
}

/// Run the formatter twice on the given file. Does not write back to the file
fn check_file(
    input_path: &Path,
    stability_check: bool,
    write: bool,
) -> Result<Statistics, CheckFileError> {
    let content = fs::read_to_string(input_path)?;
    let printed = match format_module(&content, PyFormatOptions::default()) {
        Ok(printed) => printed,
        Err(err @ (FormatModuleError::LexError(_) | FormatModuleError::ParseError(_))) => {
            return Err(CheckFileError::SyntaxErrorInInput(err));
        }
        Err(FormatModuleError::FormatError(err)) => {
            return Err(CheckFileError::FormatError(err));
        }
        Err(FormatModuleError::PrintError(err)) => {
            return Err(CheckFileError::PrintError(err));
        }
    };
    let formatted = printed.as_code();

    if write && content != formatted {
        // Simple atomic write.
        // The file is in a directory so it must have a parent. Surprisingly `DirEntry` doesn't
        // give us access without unwrap
        let mut file = NamedTempFile::new_in(input_path.parent().unwrap())?;
        file.write_all(formatted.as_bytes())?;
        // "If a file exists at the target path, persist will atomically replace it."
        file.persist(input_path).map_err(|error| error.error)?;
    }

    if stability_check {
        let reformatted = match format_module(formatted, PyFormatOptions::default()) {
            Ok(reformatted) => reformatted,
            Err(err @ (FormatModuleError::LexError(_) | FormatModuleError::ParseError(_))) => {
                return Err(CheckFileError::SyntaxErrorInOutput {
                    formatted: formatted.to_string(),
                    error: err,
                });
            }
            Err(FormatModuleError::FormatError(err)) => {
                return Err(CheckFileError::FormatError(err));
            }
            Err(FormatModuleError::PrintError(err)) => {
                return Err(CheckFileError::PrintError(err));
            }
        };

        if reformatted.as_code() != formatted {
            return Err(CheckFileError::Unstable {
                formatted: formatted.to_string(),
                reformatted: reformatted.into_code(),
            });
        }
    }

    Ok(Statistics::from_versions(&content, formatted))
}
