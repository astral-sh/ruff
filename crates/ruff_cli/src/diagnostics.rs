#![cfg_attr(target_family = "wasm", allow(dead_code))]

use std::borrow::Cow;
use std::fs::write;
use std::io;
use std::io::Write;
use std::ops::AddAssign;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use filetime::FileTime;
use log::{debug, error, warn};
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;
use similar::TextDiff;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::cache::Cache;
use ruff::jupyter::{Cell, Notebook};
use ruff::linter::{lint_fix, lint_only, FixTable, FixerResult, LinterResult};
use ruff::logging::DisplayParseError;
use ruff::message::Message;
use ruff::pyproject_toml::lint_pyproject_toml;
use ruff::registry::Rule;
use ruff::settings::{flags, AllSettings, Settings};
use ruff::source_kind::SourceKind;
use ruff::{fs, IOError};
use ruff_diagnostics::Diagnostic;
use ruff_macros::CacheKey;
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::PySourceType;
use ruff_python_stdlib::path::is_project_toml;
use ruff_source_file::{LineIndex, SourceCode, SourceFileBuilder};

#[derive(CacheKey)]
pub(crate) struct FileCacheKey {
    /// Timestamp when the file was last modified before the (cached) check.
    file_last_modified: FileTime,
    /// Permissions of the file before the (cached) check.
    file_permissions_mode: u32,
}

impl FileCacheKey {
    fn from_path(path: &Path) -> io::Result<FileCacheKey> {
        // Construct a cache key for the file
        let metadata = path.metadata()?;

        #[cfg(unix)]
        let permissions = metadata.permissions().mode();
        #[cfg(windows)]
        let permissions: u32 = metadata.permissions().readonly().into();

        Ok(FileCacheKey {
            file_last_modified: FileTime::from_last_modification_time(&metadata),
            file_permissions_mode: permissions,
        })
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Diagnostics {
    pub(crate) messages: Vec<Message>,
    pub(crate) fixed: FxHashMap<String, FixTable>,
    pub(crate) imports: ImportMap,
    pub(crate) source_kind: FxHashMap<String, SourceKind>,
}

impl Diagnostics {
    pub(crate) fn new(messages: Vec<Message>, imports: ImportMap) -> Self {
        Self {
            messages,
            fixed: FxHashMap::default(),
            imports,
            source_kind: FxHashMap::default(),
        }
    }

    /// Generate [`Diagnostics`] based on an [`io::Error`].
    pub(crate) fn from_io_error(err: &io::Error, path: &Path, settings: &Settings) -> Self {
        if settings.rules.enabled(Rule::IOError) {
            let io_err = Diagnostic::new(
                IOError {
                    message: err.to_string(),
                },
                TextRange::default(),
            );
            let dummy = SourceFileBuilder::new(path.to_string_lossy().as_ref(), "").finish();
            Self::new(
                vec![Message::from_diagnostic(io_err, dummy, TextSize::default())],
                ImportMap::default(),
            )
        } else {
            warn!(
                "{}{}{} {err}",
                "Failed to lint ".bold(),
                fs::relativize_path(path).bold(),
                ":".bold()
            );
            Self::default()
        }
    }
}

impl AddAssign for Diagnostics {
    fn add_assign(&mut self, other: Self) {
        self.messages.extend(other.messages);
        self.imports.extend(other.imports);
        for (filename, fixed) in other.fixed {
            if fixed.is_empty() {
                continue;
            }
            let fixed_in_file = self.fixed.entry(filename).or_default();
            for (rule, count) in fixed {
                if count > 0 {
                    *fixed_in_file.entry(rule).or_default() += count;
                }
            }
        }
        self.source_kind.extend(other.source_kind);
    }
}

/// Read a Jupyter Notebook from disk.
///
/// Returns either an indexed Python Jupyter Notebook or a diagnostic (which is empty if we skip).
fn notebook_from_path(path: &Path) -> Result<Notebook, Box<Diagnostics>> {
    let notebook = match Notebook::from_path(path) {
        Ok(notebook) => {
            if !notebook.is_python_notebook() {
                // Not a python notebook, this could e.g. be an R notebook which we want to just skip.
                debug!(
                    "Skipping {} because it's not a Python notebook",
                    path.display()
                );
                return Err(Box::default());
            }
            notebook
        }
        Err(diagnostic) => {
            // Failed to read the jupyter notebook
            return Err(Box::new(Diagnostics {
                messages: vec![Message::from_diagnostic(
                    *diagnostic,
                    SourceFileBuilder::new(path.to_string_lossy().as_ref(), "").finish(),
                    TextSize::default(),
                )],
                ..Diagnostics::default()
            }));
        }
    };

    Ok(notebook)
}

/// Parse a Jupyter Notebook from a JSON string.
///
/// Returns either an indexed Python Jupyter Notebook or a diagnostic (which is empty if we skip).
fn notebook_from_source_code(
    source_code: &str,
    path: Option<&Path>,
) -> Result<Notebook, Box<Diagnostics>> {
    let notebook = match Notebook::from_source_code(source_code) {
        Ok(notebook) => {
            if !notebook.is_python_notebook() {
                // Not a python notebook, this could e.g. be an R notebook which we want to just skip.
                if let Some(path) = path {
                    debug!(
                        "Skipping {} because it's not a Python notebook",
                        path.display()
                    );
                }
                return Err(Box::default());
            }
            notebook
        }
        Err(diagnostic) => {
            // Failed to read the jupyter notebook
            return Err(Box::new(Diagnostics {
                messages: vec![Message::from_diagnostic(
                    *diagnostic,
                    SourceFileBuilder::new(path.map(Path::to_string_lossy).unwrap_or_default(), "")
                        .finish(),
                    TextSize::default(),
                )],
                ..Diagnostics::default()
            }));
        }
    };

    Ok(notebook)
}

/// Lint the source code at the given `Path`.
pub(crate) fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &AllSettings,
    cache: Option<&Cache>,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    // Check the cache.
    // TODO(charlie): `fixer::Mode::Apply` and `fixer::Mode::Diff` both have
    // side-effects that aren't captured in the cache. (In practice, it's fine
    // to cache `fixer::Mode::Apply`, since a file either has no fixes, or we'll
    // write the fixes to disk, thus invalidating the cache. But it's a bit hard
    // to reason about. We need to come up with a better solution here.)
    let caching = match cache {
        Some(cache) if noqa.into() && autofix.is_generate() => {
            let relative_path = cache
                .relative_path(path)
                .expect("wrong package cache for file");

            let cache_key = FileCacheKey::from_path(path).context("Failed to create cache key")?;

            if let Some(cache) = cache.get(relative_path, &cache_key) {
                return Ok(cache.as_diagnostics(path));
            }

            // Stash the file metadata for later so when we update the cache it reflects the prerun
            // information
            Some((cache, relative_path, cache_key))
        }
        _ => None,
    };

    debug!("Checking: {}", path.display());

    // We have to special case this here since the Python tokenizer doesn't work with TOML.
    if is_project_toml(path) {
        let messages = if settings
            .lib
            .rules
            .iter_enabled()
            .any(|rule_code| rule_code.lint_source().is_pyproject_toml())
        {
            let contents = match std::fs::read_to_string(path) {
                Ok(contents) => contents,
                Err(err) => {
                    return Ok(Diagnostics::from_io_error(&err, path, &settings.lib));
                }
            };
            let source_file = SourceFileBuilder::new(path.to_string_lossy(), contents).finish();
            lint_pyproject_toml(source_file, &settings.lib)
        } else {
            vec![]
        };
        return Ok(Diagnostics {
            messages,
            ..Diagnostics::default()
        });
    }

    // Extract the sources from the file.
    let LintSources {
        source_type,
        mut source_kind,
        contents,
    } = match LintSources::try_from_path(path) {
        Ok(sources) => sources,
        Err(SourceExtractionError::Io(err)) => {
            return Ok(Diagnostics::from_io_error(&err, path, &settings.lib));
        }
        Err(SourceExtractionError::Diagnostics(diagnostics)) => {
            return Ok(*diagnostics);
        }
    };

    // Lint the file.
    let (
        LinterResult {
            data: (messages, imports),
            error: parse_error,
        },
        fixed,
    ) = if matches!(autofix, flags::FixMode::Apply | flags::FixMode::Diff) {
        if let Ok(FixerResult {
            result,
            transformed,
            fixed,
        }) = lint_fix(
            &contents,
            path,
            package,
            noqa,
            &settings.lib,
            &mut source_kind,
            source_type,
        ) {
            if !fixed.is_empty() {
                match autofix {
                    flags::FixMode::Apply => match &source_kind {
                        SourceKind::Python => {
                            write(path, transformed.as_bytes())?;
                        }
                        SourceKind::Jupyter(notebook) => {
                            notebook.write(path)?;
                        }
                    },
                    flags::FixMode::Diff => {
                        match &source_kind {
                            SourceKind::Python => {
                                let mut stdout = io::stdout().lock();
                                TextDiff::from_lines(&contents, &transformed)
                                    .unified_diff()
                                    .header(&fs::relativize_path(path), &fs::relativize_path(path))
                                    .to_writer(&mut stdout)?;
                                stdout.write_all(b"\n")?;
                                stdout.flush()?;
                            }
                            SourceKind::Jupyter(dest_notebook) => {
                                // We need to load the notebook again, since we might've
                                // mutated it.
                                let src_notebook = match notebook_from_path(path) {
                                    Ok(notebook) => notebook,
                                    Err(diagnostic) => return Ok(*diagnostic),
                                };
                                let mut stdout = io::stdout().lock();
                                for ((idx, src_cell), dest_cell) in src_notebook
                                    .cells()
                                    .iter()
                                    .enumerate()
                                    .zip(dest_notebook.cells().iter())
                                {
                                    let (Cell::Code(src_code_cell), Cell::Code(dest_code_cell)) =
                                        (src_cell, dest_cell)
                                    else {
                                        continue;
                                    };
                                    TextDiff::from_lines(
                                        &src_code_cell.source.to_string(),
                                        &dest_code_cell.source.to_string(),
                                    )
                                    .unified_diff()
                                    // Jupyter notebook cells don't necessarily have a newline
                                    // at the end. For example,
                                    //
                                    // ```python
                                    // print("hello")
                                    // ```
                                    //
                                    // For a cell containing the above code, there'll only be one line,
                                    // and it won't have a newline at the end. If it did, there'd be
                                    // two lines, and the second line would be empty:
                                    //
                                    // ```python
                                    // print("hello")
                                    //
                                    // ```
                                    .missing_newline_hint(false)
                                    .header(
                                        &format!("{}:cell {}", &fs::relativize_path(path), idx),
                                        &format!("{}:cell {}", &fs::relativize_path(path), idx),
                                    )
                                    .to_writer(&mut stdout)?;
                                }
                                stdout.write_all(b"\n")?;
                                stdout.flush()?;
                            }
                        }
                    }
                    flags::FixMode::Generate => {}
                }
            }
            (result, fixed)
        } else {
            // If we fail to autofix, lint the original source code.
            let result = lint_only(
                &contents,
                path,
                package,
                &settings.lib,
                noqa,
                Some(&source_kind),
                source_type,
            );
            let fixed = FxHashMap::default();
            (result, fixed)
        }
    } else {
        let result = lint_only(
            &contents,
            path,
            package,
            &settings.lib,
            noqa,
            Some(&source_kind),
            source_type,
        );
        let fixed = FxHashMap::default();
        (result, fixed)
    };

    let imports = imports.unwrap_or_default();

    if let Some((cache, relative_path, key)) = caching {
        // We don't cache parsing errors.
        if parse_error.is_none() {
            cache.update(relative_path.to_owned(), key, &messages, &imports);
        }
    }

    if let Some(err) = parse_error {
        error!(
            "{}",
            DisplayParseError::new(
                err,
                SourceCode::new(&contents, &LineIndex::from_source_text(&contents)),
                Some(&source_kind),
            )
        );
    }

    Ok(Diagnostics {
        messages,
        fixed: FxHashMap::from_iter([(fs::relativize_path(path), fixed)]),
        imports,
        source_kind: FxHashMap::from_iter([(
            path.to_str()
                .ok_or_else(|| anyhow!("Unable to parse filename: {:?}", path))?
                .to_string(),
            source_kind,
        )]),
    })
}

/// Generate `Diagnostic`s from source code content derived from
/// stdin.
pub(crate) fn lint_stdin(
    path: Option<&Path>,
    package: Option<&Path>,
    contents: &str,
    settings: &Settings,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    // Extract the sources from the file.
    let LintSources {
        source_type,
        mut source_kind,
        contents,
    } = match LintSources::try_from_source_code(contents, path) {
        Ok(sources) => sources,
        Err(SourceExtractionError::Io(err)) => {
            // SAFETY: An `io::Error` can only occur if we're reading from a path.
            return Ok(Diagnostics::from_io_error(&err, path.unwrap(), settings));
        }
        Err(SourceExtractionError::Diagnostics(diagnostics)) => {
            return Ok(*diagnostics);
        }
    };

    // Lint the inputs.
    let (
        LinterResult {
            data: (messages, imports),
            error: parse_error,
        },
        fixed,
    ) = if matches!(autofix, flags::FixMode::Apply | flags::FixMode::Diff) {
        if let Ok(FixerResult {
            result,
            transformed,
            fixed,
        }) = lint_fix(
            &contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            noqa,
            settings,
            &mut source_kind,
            source_type,
        ) {
            match autofix {
                flags::FixMode::Apply => {
                    // Write the contents to stdout, regardless of whether any errors were fixed.
                    io::stdout().write_all(transformed.as_bytes())?;
                }
                flags::FixMode::Diff => {
                    // But only write a diff if it's non-empty.
                    if !fixed.is_empty() {
                        let text_diff = TextDiff::from_lines(&contents, &transformed);
                        let mut unified_diff = text_diff.unified_diff();
                        if let Some(path) = path {
                            unified_diff
                                .header(&fs::relativize_path(path), &fs::relativize_path(path));
                        }

                        let mut stdout = io::stdout().lock();
                        unified_diff.to_writer(&mut stdout)?;
                        stdout.write_all(b"\n")?;
                        stdout.flush()?;
                    }
                }
                flags::FixMode::Generate => {}
            }

            (result, fixed)
        } else {
            // If we fail to autofix, lint the original source code.
            let result = lint_only(
                &contents,
                path.unwrap_or_else(|| Path::new("-")),
                package,
                settings,
                noqa,
                Some(&source_kind),
                source_type,
            );
            let fixed = FxHashMap::default();

            // Write the contents to stdout anyway.
            if autofix.is_apply() {
                io::stdout().write_all(contents.as_bytes())?;
            }

            (result, fixed)
        }
    } else {
        let result = lint_only(
            &contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
            noqa,
            Some(&source_kind),
            source_type,
        );
        let fixed = FxHashMap::default();
        (result, fixed)
    };

    let imports = imports.unwrap_or_default();

    if let Some(err) = parse_error {
        error!(
            "Failed to parse {}: {err}",
            path.map_or_else(|| "-".into(), fs::relativize_path).bold()
        );
    }

    Ok(Diagnostics {
        messages,
        fixed: FxHashMap::from_iter([(
            fs::relativize_path(path.unwrap_or_else(|| Path::new("-"))),
            fixed,
        )]),
        imports,
        source_kind: FxHashMap::default(),
    })
}

#[derive(Debug)]
struct LintSources<'a> {
    /// The "type" of source code, e.g. `.py`, `.pyi`, `.ipynb`, etc.
    source_type: PySourceType,
    /// The "kind" of source, e.g. Python file, Jupyter Notebook, etc.
    source_kind: SourceKind,
    /// The contents of the source code.
    contents: Cow<'a, str>,
}

impl<'a> LintSources<'a> {
    /// Extract the lint [`LintSources`] from the given file path.
    fn try_from_path(path: &Path) -> Result<LintSources, SourceExtractionError> {
        let source_type = PySourceType::from(path);

        // Read the file from disk.
        if source_type.is_jupyter() {
            let notebook = notebook_from_path(path).map_err(SourceExtractionError::Diagnostics)?;
            let contents = notebook.source_code().to_string();
            let source_kind = SourceKind::Jupyter(notebook);
            Ok(LintSources {
                source_type,
                source_kind,
                contents: Cow::Owned(contents),
            })
        } else {
            // This is tested by ruff_cli integration test `unreadable_file`
            let contents = std::fs::read_to_string(path).map_err(SourceExtractionError::Io)?;
            Ok(LintSources {
                source_type,
                source_kind: SourceKind::Python,
                contents: Cow::Owned(contents),
            })
        }
    }

    /// Extract the lint [`LintSources`] from the raw string contents, optionally accompanied by a
    /// file path indicating the path to the file from which the contents were read. If provided,
    /// the file path should be used for diagnostics, but not for reading the file from disk.
    fn try_from_source_code(
        source_code: &'a str,
        path: Option<&Path>,
    ) -> Result<LintSources<'a>, SourceExtractionError> {
        let source_type = path.map(PySourceType::from).unwrap_or_default();

        if source_type.is_jupyter() {
            let notebook = notebook_from_source_code(source_code, path)
                .map_err(SourceExtractionError::Diagnostics)?;
            let contents = notebook.source_code().to_string();
            let source_kind = SourceKind::Jupyter(notebook);
            Ok(LintSources {
                source_type,
                source_kind,
                contents: Cow::Owned(contents),
            })
        } else {
            Ok(LintSources {
                source_type,
                source_kind: SourceKind::Python,
                contents: Cow::Borrowed(source_code),
            })
        }
    }
}

#[derive(Debug)]
enum SourceExtractionError {
    /// The extraction failed due to an [`io::Error`].
    Io(io::Error),
    /// The extraction failed, and generated [`Diagnostics`] to report.
    Diagnostics(Box<Diagnostics>),
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::diagnostics::{notebook_from_path, notebook_from_source_code, Diagnostics};

    #[test]
    fn test_r() {
        let path = Path::new("../ruff/resources/test/fixtures/jupyter/R.ipynb");
        // No diagnostics is used as skip signal.
        assert_eq!(
            notebook_from_path(path).unwrap_err(),
            Box::<Diagnostics>::default()
        );

        let contents = std::fs::read_to_string(path).unwrap();
        // No diagnostics is used as skip signal.
        assert_eq!(
            notebook_from_source_code(&contents, Some(path)).unwrap_err(),
            Box::<Diagnostics>::default()
        );
    }
}
