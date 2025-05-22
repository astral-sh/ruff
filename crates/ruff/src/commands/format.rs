use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::io::{Write, stderr, stdout};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use itertools::Itertools;
use log::{error, warn};
use rayon::iter::Either::{Left, Right};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ruff_python_parser::ParseError;
use rustc_hash::FxHashSet;
use thiserror::Error;
use tracing::debug;

use ruff_db::panic::{PanicError, catch_unwind};
use ruff_diagnostics::SourceMap;
use ruff_linter::fs;
use ruff_linter::logging::{DisplayParseError, LogLevel};
use ruff_linter::package::PackageRoot;
use ruff_linter::registry::Rule;
use ruff_linter::rules::flake8_quotes::settings::Quote;
use ruff_linter::source_kind::{SourceError, SourceKind};
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_python_formatter::{FormatModuleError, QuoteStyle, format_module_source, format_range};
use ruff_source_file::LineIndex;
use ruff_text_size::{TextLen, TextRange, TextSize};
use ruff_workspace::FormatterSettings;
use ruff_workspace::resolver::{ResolvedFile, Resolver, match_exclusion, python_files_in_path};

use crate::args::{ConfigArguments, FormatArguments, FormatRange};
use crate::cache::{Cache, FileCacheKey, PackageCacheMap, PackageCaches};
use crate::resolve::resolve;
use crate::{ExitStatus, resolve_default_files};

#[derive(Debug, Copy, Clone, is_macro::Is)]
pub(crate) enum FormatMode {
    /// Write the formatted contents back to the file.
    Write,
    /// Check if the file is formatted, but do not write the formatted contents back.
    Check,
    /// Check if the file is formatted, show a diff if not.
    Diff,
}

impl FormatMode {
    pub(crate) fn from_cli(cli: &FormatArguments) -> Self {
        if cli.diff {
            FormatMode::Diff
        } else if cli.check {
            FormatMode::Check
        } else {
            FormatMode::Write
        }
    }
}

/// Format a set of files, and return the exit status.
pub(crate) fn format(
    cli: FormatArguments,
    config_arguments: &ConfigArguments,
) -> Result<ExitStatus> {
    let pyproject_config = resolve(config_arguments, cli.stdin_filename.as_deref())?;
    let mode = FormatMode::from_cli(&cli);
    let files = resolve_default_files(cli.files, false);
    let (paths, resolver) = python_files_in_path(&files, &pyproject_config, config_arguments)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(ExitStatus::Success);
    }

    if cli.range.is_some() && paths.len() > 1 {
        return Err(anyhow::anyhow!(
            "The `--range` option is only supported when formatting a single file but the specified paths resolve to {} files.",
            paths.len()
        ));
    }

    warn_incompatible_formatter_settings(&resolver);

    // Discover the package root for each Python file.
    let package_roots = resolver.package_roots(
        &paths
            .iter()
            .flatten()
            .map(ResolvedFile::path)
            .collect::<Vec<_>>(),
    );

    let caches = if cli.no_cache {
        None
    } else {
        // `--no-cache` doesn't respect code changes, and so is often confusing during
        // development.
        #[cfg(debug_assertions)]
        crate::warn_user!("Detected debug build without --no-cache.");

        Some(PackageCacheMap::init(&package_roots, &resolver))
    };

    let start = Instant::now();
    let (results, mut errors): (Vec<_>, Vec<_>) = paths
        .par_iter()
        .filter_map(|entry| {
            match entry {
                Ok(resolved_file) => {
                    let path = resolved_file.path();
                    let settings = resolver.resolve(path);

                    let source_type = match settings.formatter.extension.get(path) {
                        None => match SourceType::from(path) {
                            SourceType::Python(source_type) => source_type,
                            SourceType::Toml(_) => {
                                // Ignore any non-Python files.
                                return None;
                            }
                        },
                        Some(language) => PySourceType::from(language),
                    };

                    // Ignore files that are excluded from formatting
                    if (settings.file_resolver.force_exclude || !resolved_file.is_root())
                        && match_exclusion(
                            path,
                            resolved_file.file_name(),
                            &settings.formatter.exclude,
                        )
                    {
                        return None;
                    }

                    let package = path
                        .parent()
                        .and_then(|parent| package_roots.get(parent).copied())
                        .flatten();
                    let cache_root = package
                        .map(PackageRoot::path)
                        .unwrap_or_else(|| path.parent().unwrap_or(path));
                    let cache = caches.get(cache_root);

                    Some(
                        match catch_unwind(|| {
                            format_path(
                                path,
                                &settings.formatter,
                                source_type,
                                mode,
                                cli.range,
                                cache,
                            )
                        }) {
                            Ok(inner) => inner.map(|result| FormatPathResult {
                                path: resolved_file.path().to_path_buf(),
                                result,
                            }),
                            Err(error) => Err(FormatCommandError::Panic(
                                Some(resolved_file.path().to_path_buf()),
                                Box::new(error),
                            )),
                        },
                    )
                }
                Err(err) => Some(Err(FormatCommandError::Ignore(err.clone()))),
            }
        })
        .partition_map(|result| match result {
            Ok(diagnostic) => Left(diagnostic),
            Err(err) => Right(err),
        });
    let duration = start.elapsed();

    debug!(
        "Formatted {} files in {:.2?}",
        results.len() + errors.len(),
        duration
    );

    // Store the caches.
    caches.persist()?;

    // Report on any errors.
    errors.sort_unstable_by(|a, b| a.path().cmp(&b.path()));

    for error in &errors {
        error!("{error}");
    }

    let results = FormatResults::new(results.as_slice(), mode);
    match mode {
        FormatMode::Write => {}
        FormatMode::Check => {
            results.write_changed(&mut stdout().lock())?;
        }
        FormatMode::Diff => {
            results.write_diff(&mut stdout().lock())?;
        }
    }

    // Report on the formatting changes.
    if config_arguments.log_level >= LogLevel::Default {
        if mode.is_diff() {
            // Allow piping the diff to e.g. a file by writing the summary to stderr
            results.write_summary(&mut stderr().lock())?;
        } else {
            results.write_summary(&mut stdout().lock())?;
        }
    }

    match mode {
        FormatMode::Write => {
            if errors.is_empty() {
                if cli.exit_non_zero_on_format && results.any_formatted() {
                    Ok(ExitStatus::Failure)
                } else {
                    Ok(ExitStatus::Success)
                }
            } else {
                Ok(ExitStatus::Error)
            }
        }
        FormatMode::Check | FormatMode::Diff => {
            if errors.is_empty() {
                if results.any_formatted() {
                    Ok(ExitStatus::Failure)
                } else {
                    Ok(ExitStatus::Success)
                }
            } else {
                Ok(ExitStatus::Error)
            }
        }
    }
}

/// Format the file at the given [`Path`].
#[tracing::instrument(level = "debug", skip_all, fields(path = %path.display()))]
pub(crate) fn format_path(
    path: &Path,
    settings: &FormatterSettings,
    source_type: PySourceType,
    mode: FormatMode,
    range: Option<FormatRange>,
    cache: Option<&Cache>,
) -> Result<FormatResult, FormatCommandError> {
    if let Some(cache) = cache {
        let relative_path = cache
            .relative_path(path)
            .expect("wrong package cache for file");

        if let Ok(cache_key) = FileCacheKey::from_path(path) {
            if cache.is_formatted(relative_path, &cache_key) {
                return Ok(FormatResult::Unchanged);
            }
        }
    }

    // Extract the sources from the file.
    let unformatted = match SourceKind::from_path(path, source_type) {
        Ok(Some(source_kind)) => source_kind,
        // Non-Python Jupyter notebook.
        Ok(None) => return Ok(FormatResult::Skipped),
        Err(err) => {
            return Err(FormatCommandError::Read(Some(path.to_path_buf()), err));
        }
    };

    // Don't write back to the cache if formatting a range.
    let cache = cache.filter(|_| range.is_none());

    // Format the source.
    let format_result = match format_source(&unformatted, source_type, Some(path), settings, range)?
    {
        FormattedSource::Formatted(formatted) => match mode {
            FormatMode::Write => {
                let mut writer = File::create(path).map_err(|err| {
                    FormatCommandError::Write(Some(path.to_path_buf()), err.into())
                })?;
                formatted
                    .write(&mut writer)
                    .map_err(|err| FormatCommandError::Write(Some(path.to_path_buf()), err))?;

                if let Some(cache) = cache {
                    if let Ok(cache_key) = FileCacheKey::from_path(path) {
                        let relative_path = cache
                            .relative_path(path)
                            .expect("wrong package cache for file");
                        cache.set_formatted(relative_path.to_path_buf(), &cache_key);
                    }
                }

                FormatResult::Formatted
            }
            FormatMode::Check => FormatResult::Formatted,
            FormatMode::Diff => FormatResult::Diff {
                unformatted,
                formatted,
            },
        },
        FormattedSource::Unchanged => {
            if let Some(cache) = cache {
                if let Ok(cache_key) = FileCacheKey::from_path(path) {
                    let relative_path = cache
                        .relative_path(path)
                        .expect("wrong package cache for file");
                    cache.set_formatted(relative_path.to_path_buf(), &cache_key);
                }
            }

            FormatResult::Unchanged
        }
    };

    Ok(format_result)
}

#[derive(Debug)]
pub(crate) enum FormattedSource {
    /// The source was formatted, and the [`SourceKind`] contains the transformed source code.
    Formatted(SourceKind),
    /// The source was unchanged.
    Unchanged,
}

impl From<FormattedSource> for FormatResult {
    fn from(value: FormattedSource) -> Self {
        match value {
            FormattedSource::Formatted(_) => FormatResult::Formatted,
            FormattedSource::Unchanged => FormatResult::Unchanged,
        }
    }
}

/// Format a [`SourceKind`], returning the transformed [`SourceKind`], or `None` if the source was
/// unchanged.
pub(crate) fn format_source(
    source_kind: &SourceKind,
    source_type: PySourceType,
    path: Option<&Path>,
    settings: &FormatterSettings,
    range: Option<FormatRange>,
) -> Result<FormattedSource, FormatCommandError> {
    match &source_kind {
        SourceKind::Python(unformatted) => {
            let options = settings.to_format_options(source_type, unformatted, path);

            let formatted = if let Some(range) = range {
                let line_index = LineIndex::from_source_text(unformatted);
                let byte_range = range.to_text_range(unformatted, &line_index);
                format_range(unformatted, byte_range, options).map(|formatted_range| {
                    let mut formatted = unformatted.to_string();
                    formatted.replace_range(
                        std::ops::Range::<usize>::from(formatted_range.source_range()),
                        formatted_range.as_code(),
                    );

                    formatted
                })
            } else {
                // Using `Printed::into_code` requires adding `ruff_formatter` as a direct dependency, and I suspect that Rust can optimize the closure away regardless.
                #[expect(clippy::redundant_closure_for_method_calls)]
                format_module_source(unformatted, options).map(|formatted| formatted.into_code())
            };

            let formatted = formatted.map_err(|err| {
                if let FormatModuleError::ParseError(err) = err {
                    DisplayParseError::from_source_kind(
                        err,
                        path.map(Path::to_path_buf),
                        source_kind,
                    )
                    .into()
                } else {
                    FormatCommandError::Format(path.map(Path::to_path_buf), err)
                }
            })?;

            if formatted.len() == unformatted.len() && formatted == *unformatted {
                Ok(FormattedSource::Unchanged)
            } else {
                Ok(FormattedSource::Formatted(SourceKind::Python(formatted)))
            }
        }
        SourceKind::IpyNotebook(notebook) => {
            if !notebook.is_python_notebook() {
                return Ok(FormattedSource::Unchanged);
            }

            if range.is_some() {
                return Err(FormatCommandError::RangeFormatNotebook(
                    path.map(Path::to_path_buf),
                ));
            }

            let options = settings.to_format_options(source_type, notebook.source_code(), path);

            let mut output: Option<String> = None;
            let mut last: Option<TextSize> = None;
            let mut source_map = SourceMap::default();

            // Format each cell individually.
            for (start, end) in notebook.cell_offsets().iter().tuple_windows::<(_, _)>() {
                let range = TextRange::new(*start, *end);
                let unformatted = &notebook.source_code()[range];

                // Format the cell.
                let formatted =
                    format_module_source(unformatted, options.clone()).map_err(|err| {
                        if let FormatModuleError::ParseError(err) = err {
                            // Offset the error by the start of the cell
                            DisplayParseError::from_source_kind(
                                ParseError {
                                    error: err.error,
                                    location: err.location.checked_add(*start).unwrap(),
                                },
                                path.map(Path::to_path_buf),
                                source_kind,
                            )
                            .into()
                        } else {
                            FormatCommandError::Format(path.map(Path::to_path_buf), err)
                        }
                    })?;

                // If the cell is unchanged, skip it.
                let formatted = formatted.as_code();
                if formatted.len() == unformatted.len() && formatted == unformatted {
                    continue;
                }

                // If this is the first newly-formatted cell, initialize the output.
                let output = output
                    .get_or_insert_with(|| String::with_capacity(notebook.source_code().len()));

                // Add all contents from `last` to the current cell.
                let slice = &notebook.source_code()
                    [TextRange::new(last.unwrap_or_default(), range.start())];
                output.push_str(slice);

                // Add the start source marker for the cell.
                source_map.push_marker(*start, output.text_len());

                // Add the cell itself.
                output.push_str(formatted);

                // Add the end source marker for the added cell.
                source_map.push_marker(*end, output.text_len());

                // Track that the cell was formatted.
                last = Some(*end);
            }

            // If the file was unchanged, return `None`.
            let (Some(mut output), Some(last)) = (output, last) else {
                return Ok(FormattedSource::Unchanged);
            };

            // Add the remaining content.
            let slice = &notebook.source_code()[usize::from(last)..];
            output.push_str(slice);

            // Update the notebook.
            let mut formatted = notebook.clone();
            formatted.update(&source_map, output);

            Ok(FormattedSource::Formatted(SourceKind::IpyNotebook(
                formatted,
            )))
        }
    }
}

/// The result of an individual formatting operation.
#[derive(Debug, Clone, is_macro::Is)]
pub(crate) enum FormatResult {
    /// The file was formatted.
    Formatted,

    /// The file was formatted, [`SourceKind`] contains the formatted code
    Diff {
        unformatted: SourceKind,
        formatted: SourceKind,
    },

    /// The file was unchanged, as the formatted contents matched the existing contents.
    Unchanged,

    /// Skipped formatting because its an unsupported file format
    Skipped,
}

/// The coupling of a [`FormatResult`] with the path of the file that was analyzed.
#[derive(Debug)]
struct FormatPathResult {
    path: PathBuf,
    result: FormatResult,
}

/// The results of formatting a set of files
#[derive(Debug)]
struct FormatResults<'a> {
    /// The individual formatting results.
    results: &'a [FormatPathResult],
    /// The format mode that was used.
    mode: FormatMode,
}

impl<'a> FormatResults<'a> {
    fn new(results: &'a [FormatPathResult], mode: FormatMode) -> Self {
        Self { results, mode }
    }

    /// Returns `true` if any of the files require formatting.
    fn any_formatted(&self) -> bool {
        self.results.iter().any(|result| match result.result {
            FormatResult::Formatted | FormatResult::Diff { .. } => true,
            FormatResult::Unchanged | FormatResult::Skipped => false,
        })
    }

    /// Write a diff of the formatting changes to the given writer.
    fn write_diff(&self, f: &mut impl Write) -> io::Result<()> {
        for (path, unformatted, formatted) in self
            .results
            .iter()
            .filter_map(|result| {
                if let FormatResult::Diff {
                    unformatted,
                    formatted,
                } = &result.result
                {
                    Some((result.path.as_path(), unformatted, formatted))
                } else {
                    None
                }
            })
            .sorted_unstable_by_key(|(path, _, _)| *path)
        {
            write!(f, "{}", unformatted.diff(formatted, Some(path)).unwrap())?;
        }

        Ok(())
    }

    /// Write a list of the files that would be changed to the given writer.
    fn write_changed(&self, f: &mut impl Write) -> io::Result<()> {
        for path in self
            .results
            .iter()
            .filter_map(|result| {
                if result.result.is_formatted() {
                    Some(result.path.as_path())
                } else {
                    None
                }
            })
            .sorted_unstable()
        {
            writeln!(f, "Would reformat: {}", fs::relativize_path(path).bold())?;
        }

        Ok(())
    }

    /// Write a summary of the formatting results to the given writer.
    fn write_summary(&self, f: &mut impl Write) -> io::Result<()> {
        // Compute the number of changed and unchanged files.
        let mut changed = 0u32;
        let mut unchanged = 0u32;
        for result in self.results {
            match &result.result {
                FormatResult::Formatted => {
                    changed += 1;
                }
                FormatResult::Unchanged => unchanged += 1,
                FormatResult::Diff { .. } => {
                    changed += 1;
                }
                FormatResult::Skipped => {}
            }
        }

        // Write out a summary of the formatting results.
        if changed > 0 && unchanged > 0 {
            writeln!(
                f,
                "{} file{} {}, {} file{} {}",
                changed,
                if changed == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check | FormatMode::Diff => "would be reformatted",
                },
                unchanged,
                if unchanged == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "left unchanged",
                    FormatMode::Check | FormatMode::Diff => "already formatted",
                },
            )
        } else if changed > 0 {
            writeln!(
                f,
                "{} file{} {}",
                changed,
                if changed == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check | FormatMode::Diff => "would be reformatted",
                }
            )
        } else if unchanged > 0 {
            writeln!(
                f,
                "{} file{} {}",
                unchanged,
                if unchanged == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "left unchanged",
                    FormatMode::Check | FormatMode::Diff => "already formatted",
                },
            )
        } else {
            Ok(())
        }
    }
}

/// An error that can occur while formatting a set of files.
#[derive(Error, Debug)]
pub(crate) enum FormatCommandError {
    Ignore(#[from] ignore::Error),
    Parse(#[from] DisplayParseError),
    Panic(Option<PathBuf>, Box<PanicError>),
    Read(Option<PathBuf>, SourceError),
    Format(Option<PathBuf>, FormatModuleError),
    Write(Option<PathBuf>, SourceError),
    Diff(Option<PathBuf>, io::Error),
    RangeFormatNotebook(Option<PathBuf>),
}

impl FormatCommandError {
    fn path(&self) -> Option<&Path> {
        match self {
            Self::Ignore(err) => {
                if let ignore::Error::WithPath { path, .. } = err {
                    Some(path.as_path())
                } else {
                    None
                }
            }
            Self::Parse(err) => err.path(),
            Self::Panic(path, _)
            | Self::Read(path, _)
            | Self::Format(path, _)
            | Self::Write(path, _)
            | Self::Diff(path, _)
            | Self::RangeFormatNotebook(path) => path.as_deref(),
        }
    }
}

impl Display for FormatCommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore(err) => {
                if let ignore::Error::WithPath { path, .. } = err {
                    write!(
                        f,
                        "{}{}{} {}",
                        "Failed to format ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold(),
                        err.io_error()
                            .map_or_else(|| err.to_string(), std::string::ToString::to_string)
                    )
                } else {
                    write!(
                        f,
                        "{header} {error}",
                        header = "Encountered error:".bold(),
                        error = err
                            .io_error()
                            .map_or_else(|| err.to_string(), std::string::ToString::to_string)
                    )
                }
            }
            Self::Parse(err) => {
                write!(f, "{err}")
            }
            Self::Read(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to read ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(f, "{header} {err}", header = "Failed to read:".bold())
                }
            }
            Self::Write(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to write ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(f, "{header} {err}", header = "Failed to write:".bold())
                }
            }
            Self::Format(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to format ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(f, "{header} {err}", header = "Failed to format:".bold())
                }
            }
            Self::Diff(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to generate diff for ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(
                        f,
                        "{header} {err}",
                        header = "Failed to generate diff:".bold(),
                    )
                }
            }
            Self::RangeFormatNotebook(path) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{header}{path}{colon} Range formatting isn't supported for notebooks.",
                        header = "Failed to format ".bold(),
                        path = fs::relativize_path(path).bold(),
                        colon = ":".bold()
                    )
                } else {
                    write!(
                        f,
                        "{header} Range formatting isn't supported for notebooks",
                        header = "Failed to format:".bold()
                    )
                }
            }
            Self::Panic(path, err) => {
                let message = r"This indicates a bug in Ruff. If you could open an issue at:

    https://github.com/astral-sh/ruff/issues/new?title=%5BFormatter%20panic%5D

...with the relevant file contents, the `pyproject.toml` settings, and the following stack trace, we'd be very appreciative!
";
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {message}\n{err}",
                        "Panicked while formatting ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(
                        f,
                        "{} {message}\n{err}",
                        "Panicked while formatting.".bold()
                    )
                }
            }
        }
    }
}

pub(super) fn warn_incompatible_formatter_settings(resolver: &Resolver) {
    // First, collect all rules that are incompatible regardless of the linter-specific settings.
    let mut incompatible_rules = FxHashSet::default();
    for setting in resolver.settings() {
        for rule in [
            // Flags missing trailing commas when all arguments are on its own line:
            // ```python
            // def args(
            //     aaaaaaaa, bbbbbbbbb, cccccccccc, ddddddddd, eeeeeeee, ffffff, gggggggggggg, hhhh
            // ):
            //     pass
            // ```
            Rule::MissingTrailingComma,
            // The formatter always removes blank lines before the docstring.
            Rule::IncorrectBlankLineBeforeClass,
        ] {
            if setting.linter.rules.enabled(rule) {
                incompatible_rules.insert(rule);
            }
        }
    }

    if !incompatible_rules.is_empty() {
        let mut rule_names: Vec<_> = incompatible_rules
            .into_iter()
            .map(|rule| format!("`{}`", rule.noqa_code()))
            .collect();
        rule_names.sort();
        if let [rule] = rule_names.as_slice() {
            warn_user_once!(
                "The following rule may cause conflicts when used with the formatter: {rule}. To avoid unexpected behavior, we recommend disabling this rule, either by removing it from the `lint.select` or `lint.extend-select` configuration, or adding it to the `lint.ignore` configuration."
            );
        } else {
            warn_user_once!(
                "The following rules may cause conflicts when used with the formatter: {}. To avoid unexpected behavior, we recommend disabling these rules, either by removing them from the `lint.select` or `lint.extend-select` configuration, or adding them to the `lint.ignore` configuration.",
                rule_names.join(", ")
            );
        }
    }

    // Next, validate settings-specific incompatibilities.
    for setting in resolver.settings() {
        // Validate all rules that rely on tab styles.
        if setting.linter.rules.enabled(Rule::TabIndentation)
            && setting.formatter.indent_style.is_tab()
        {
            warn_user_once!(
                "The `format.indent-style=\"tab\"` option is incompatible with `W191`, which lints against all uses of tabs. We recommend disabling these rules when using the formatter, which enforces a consistent indentation style. Alternatively, set the `format.indent-style` option to `\"space\"`."
            );
        }

        if !setting
            .linter
            .rules
            .enabled(Rule::SingleLineImplicitStringConcatenation)
            && setting
                .linter
                .rules
                .enabled(Rule::MultiLineImplicitStringConcatenation)
            && !setting.linter.flake8_implicit_str_concat.allow_multiline
        {
            warn_user_once!(
                "The `lint.flake8-implicit-str-concat.allow-multiline = false` option is incompatible with the formatter unless `ISC001` is enabled. We recommend enabling `ISC001` or setting `allow-multiline=true`."
            );
        }

        // Validate all rules that rely on tab styles.
        if setting.linter.rules.enabled(Rule::DocstringTabIndentation)
            && setting.formatter.indent_style.is_tab()
        {
            warn_user_once!(
                "The `format.indent-style=\"tab\"` option is incompatible with `D206`, with requires space-based indentation. We recommend disabling these rules when using the formatter, which enforces a consistent indentation style. Alternatively, set the `format.indent-style` option to `\"space\"`."
            );
        }

        // Validate all rules that rely on custom indent widths.
        if setting.linter.rules.any_enabled(&[
            Rule::IndentationWithInvalidMultiple,
            Rule::IndentationWithInvalidMultipleComment,
        ]) && setting.formatter.indent_width.value() != 4
        {
            warn_user_once!(
                "The `format.indent-width` option with a value other than 4 is incompatible with `E111` and `E114`. We recommend disabling these rules when using the formatter, which enforces a consistent indentation width. Alternatively, set the `format.indent-width` option to `4`."
            );
        }

        // Validate all rules that rely on quote styles.
        if setting
            .linter
            .rules
            .any_enabled(&[Rule::BadQuotesInlineString, Rule::AvoidableEscapedQuote])
        {
            match (
                setting.linter.flake8_quotes.inline_quotes,
                setting.formatter.quote_style,
            ) {
                (Quote::Double, QuoteStyle::Single) => {
                    warn_user_once!(
                        "The `flake8-quotes.inline-quotes=\"double\"` option is incompatible with the formatter's `format.quote-style=\"single\"`. We recommend disabling `Q000` and `Q003` when using the formatter, which enforces a consistent quote style. Alternatively, set both options to either `\"single\"` or `\"double\"`."
                    );
                }
                (Quote::Single, QuoteStyle::Double) => {
                    warn_user_once!(
                        "The `flake8-quotes.inline-quotes=\"single\"` option is incompatible with the formatter's `format.quote-style=\"double\"`. We recommend disabling `Q000` and `Q003` when using the formatter, which enforces a consistent quote style. Alternatively, set both options to either `\"single\"` or `\"double\"`."
                    );
                }
                _ => {}
            }
        }

        if setting.linter.rules.enabled(Rule::BadQuotesMultilineString)
            && setting.linter.flake8_quotes.multiline_quotes == Quote::Single
            && matches!(
                setting.formatter.quote_style,
                QuoteStyle::Single | QuoteStyle::Double
            )
        {
            warn_user_once!(
                "The `flake8-quotes.multiline-quotes=\"single\"` option is incompatible with the formatter. We recommend disabling `Q001` when using the formatter, which enforces double quotes for multiline strings. Alternatively, set the `flake8-quotes.multiline-quotes` option to `\"double\"`.`"
            );
        }

        if setting.linter.rules.enabled(Rule::BadQuotesDocstring)
            && setting.linter.flake8_quotes.docstring_quotes == Quote::Single
            && matches!(
                setting.formatter.quote_style,
                QuoteStyle::Single | QuoteStyle::Double
            )
        {
            warn_user_once!(
                "The `flake8-quotes.docstring-quotes=\"single\"` option is incompatible with the formatter. We recommend disabling `Q002` when using the formatter, which enforces double quotes for docstrings. Alternatively, set the `flake8-quotes.docstring-quotes` option to `\"double\"`.`"
            );
        }

        // Validate all isort settings.
        if setting.linter.rules.enabled(Rule::UnsortedImports) {
            // The formatter removes empty lines if the value is larger than 2 but always inserts a empty line after imports.
            // Two empty lines are okay because `isort` only uses this setting for top-level imports (not in nested blocks).
            if !matches!(setting.linter.isort.lines_after_imports, 1 | 2 | -1) {
                warn_user_once!(
                    "The isort option `isort.lines-after-imports` with a value other than `-1`, `1` or `2` is incompatible with the formatter. To avoid unexpected behavior, we recommend setting the option to one of: `2`, `1`, or `-1` (default)."
                );
            }

            // Values larger than two get reduced to one line by the formatter if the import is in a nested block.
            if setting.linter.isort.lines_between_types > 1 {
                warn_user_once!(
                    "The isort option `isort.lines-between-types` with a value greater than 1 is incompatible with the formatter. To avoid unexpected behavior, we recommend setting the option to one of: `1` or `0` (default)."
                );
            }

            // isort inserts a trailing comma which the formatter preserves, but only if `skip-magic-trailing-comma` isn't false.
            // This isn't relevant when using `force-single-line`, since isort will never include a trailing comma in that case.
            if setting.formatter.magic_trailing_comma.is_ignore()
                && !setting.linter.isort.force_single_line
            {
                if setting.linter.isort.force_wrap_aliases {
                    warn_user_once!(
                        "The isort option `isort.force-wrap-aliases` is incompatible with the formatter `format.skip-magic-trailing-comma=true` option. To avoid unexpected behavior, we recommend either setting `isort.force-wrap-aliases=false` or `format.skip-magic-trailing-comma=false`."
                    );
                }

                if setting.linter.isort.split_on_trailing_comma {
                    warn_user_once!(
                        "The isort option `isort.split-on-trailing-comma` is incompatible with the formatter `format.skip-magic-trailing-comma=true` option. To avoid unexpected behavior, we recommend either setting `isort.split-on-trailing-comma=false` or `format.skip-magic-trailing-comma=false`."
                    );
                }
            }
        }
    }
}
