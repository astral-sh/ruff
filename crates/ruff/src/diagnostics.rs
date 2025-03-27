#![cfg_attr(target_family = "wasm", allow(dead_code))]

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::io::Write;
use std::ops::{Add, AddAssign};
use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;
use log::{debug, warn};
use rustc_hash::FxHashMap;

use ruff_diagnostics::Diagnostic;
use ruff_linter::codes::Rule;
use ruff_linter::linter::{lint_fix, lint_only, FixTable, FixerResult, LinterResult, ParseSource};
use ruff_linter::message::{Message, SyntaxErrorMessage};
use ruff_linter::package::PackageRoot;
use ruff_linter::pyproject_toml::lint_pyproject_toml;
use ruff_linter::settings::types::UnsafeFixes;
use ruff_linter::settings::{flags, LinterSettings};
use ruff_linter::source_kind::{SourceError, SourceKind};
use ruff_linter::{fs, IOError};
use ruff_notebook::{Notebook, NotebookError, NotebookIndex};
use ruff_python_ast::{PySourceType, SourceType, TomlSourceType};
use ruff_source_file::SourceFileBuilder;
use ruff_text_size::{TextRange, TextSize};
use ruff_workspace::Settings;

use crate::cache::{Cache, FileCacheKey, LintCacheData};

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Diagnostics {
    pub(crate) messages: Vec<Message>,
    pub(crate) fixed: FixMap,
    pub(crate) notebook_indexes: FxHashMap<String, NotebookIndex>,
}

impl Diagnostics {
    pub(crate) fn new(
        messages: Vec<Message>,
        notebook_indexes: FxHashMap<String, NotebookIndex>,
    ) -> Self {
        Self {
            messages,
            fixed: FixMap::default(),
            notebook_indexes,
        }
    }

    /// Generate [`Diagnostics`] based on a [`SourceError`].
    pub(crate) fn from_source_error(
        err: &SourceError,
        path: Option<&Path>,
        settings: &LinterSettings,
    ) -> Self {
        match err {
            // IO errors.
            SourceError::Io(_)
            | SourceError::Notebook(NotebookError::Io(_) | NotebookError::Json(_)) => {
                if settings.rules.enabled(Rule::IOError) {
                    let name = path.map_or_else(|| "-".into(), Path::to_string_lossy);
                    let source_file = SourceFileBuilder::new(name, "").finish();
                    Self::new(
                        vec![Message::from_diagnostic(
                            Diagnostic::new(
                                IOError {
                                    message: err.to_string(),
                                },
                                TextRange::default(),
                            ),
                            source_file,
                            TextSize::default(),
                        )],
                        FxHashMap::default(),
                    )
                } else {
                    match path {
                        Some(path) => {
                            warn!(
                                "{}{}{} {err}",
                                "Failed to lint ".bold(),
                                fs::relativize_path(path).bold(),
                                ":".bold()
                            );
                        }
                        None => {
                            warn!("{}{} {err}", "Failed to lint".bold(), ":".bold());
                        }
                    }

                    Self::default()
                }
            }
            // Syntax errors.
            SourceError::Notebook(
                NotebookError::InvalidJson(_)
                | NotebookError::InvalidSchema(_)
                | NotebookError::InvalidFormat(_),
            ) => {
                let name = path.map_or_else(|| "-".into(), Path::to_string_lossy);
                let dummy = SourceFileBuilder::new(name, "").finish();
                Self::new(
                    vec![Message::SyntaxError(SyntaxErrorMessage {
                        message: err.to_string(),
                        range: TextRange::default(),
                        file: dummy,
                    })],
                    FxHashMap::default(),
                )
            }
        }
    }
}

impl Add for Diagnostics {
    type Output = Diagnostics;

    fn add(mut self, other: Self) -> Self::Output {
        self += other;
        self
    }
}

impl AddAssign for Diagnostics {
    fn add_assign(&mut self, other: Self) {
        self.messages.extend(other.messages);
        self.fixed += other.fixed;
        self.notebook_indexes.extend(other.notebook_indexes);
    }
}

/// A collection of fixes indexed by file path.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct FixMap(FxHashMap<String, FixTable>);

impl FixMap {
    /// Returns `true` if there are no fixes in the map.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the fixes in the map, along with the file path.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &FixTable)> {
        self.0.iter()
    }

    /// Returns an iterator over the fixes in the map.
    pub(crate) fn values(&self) -> impl Iterator<Item = &FixTable> {
        self.0.values()
    }
}

impl FromIterator<(String, FixTable)> for FixMap {
    fn from_iter<T: IntoIterator<Item = (String, FixTable)>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .filter(|(_, fixes)| !fixes.is_empty())
                .collect(),
        )
    }
}

impl AddAssign for FixMap {
    fn add_assign(&mut self, rhs: Self) {
        for (filename, fixed) in rhs.0 {
            if fixed.is_empty() {
                continue;
            }
            let fixed_in_file = self.0.entry(filename).or_default();
            for (rule, count) in fixed {
                if count > 0 {
                    *fixed_in_file.entry(rule).or_default() += count;
                }
            }
        }
    }
}

/// Lint the source code at the given `Path`.
pub(crate) fn lint_path(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    settings: &LinterSettings,
    cache: Option<&Cache>,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
    unsafe_fixes: UnsafeFixes,
) -> Result<Diagnostics> {
    // Check the cache.
    let caching = match cache {
        Some(cache) if noqa.is_enabled() => {
            let relative_path = cache
                .relative_path(path)
                .expect("wrong package cache for file");

            let cache_key = FileCacheKey::from_path(path).context("Failed to create cache key")?;
            let cached_diagnostics = cache
                .get(relative_path, &cache_key)
                .and_then(|entry| entry.to_diagnostics(path));
            if let Some(diagnostics) = cached_diagnostics {
                // `FixMode::Generate` and `FixMode::Diff` rely on side-effects (writing to disk,
                // and writing the diff to stdout, respectively). If a file has diagnostics, we
                // need to avoid reading from and writing to the cache in these modes.
                if match fix_mode {
                    flags::FixMode::Generate => true,
                    flags::FixMode::Apply | flags::FixMode::Diff => {
                        diagnostics.messages.is_empty() && diagnostics.fixed.is_empty()
                    }
                } {
                    return Ok(diagnostics);
                }
            }

            // Stash the file metadata for later so when we update the cache it reflects the prerun
            // information
            Some((cache, relative_path, cache_key))
        }
        _ => None,
    };

    debug!("Checking: {}", path.display());

    let source_type = match settings.extension.get(path).map(PySourceType::from) {
        Some(source_type) => source_type,
        None => match SourceType::from(path) {
            SourceType::Toml(TomlSourceType::Pyproject) => {
                let messages = if settings
                    .rules
                    .iter_enabled()
                    .any(|rule_code| rule_code.lint_source().is_pyproject_toml())
                {
                    let contents = match std::fs::read_to_string(path).map_err(SourceError::from) {
                        Ok(contents) => contents,
                        Err(err) => {
                            return Ok(Diagnostics::from_source_error(&err, Some(path), settings));
                        }
                    };
                    let source_file =
                        SourceFileBuilder::new(path.to_string_lossy(), contents).finish();
                    lint_pyproject_toml(source_file, settings)
                } else {
                    vec![]
                };
                return Ok(Diagnostics {
                    messages,
                    ..Diagnostics::default()
                });
            }
            SourceType::Toml(_) => return Ok(Diagnostics::default()),
            SourceType::Python(source_type) => source_type,
        },
    };

    // Extract the sources from the file.
    let source_kind = match SourceKind::from_path(path, source_type) {
        Ok(Some(source_kind)) => source_kind,
        Ok(None) => return Ok(Diagnostics::default()),
        Err(err) => {
            return Ok(Diagnostics::from_source_error(&err, Some(path), settings));
        }
    };

    // Lint the file.
    let (result, transformed, fixed) =
        if matches!(fix_mode, flags::FixMode::Apply | flags::FixMode::Diff) {
            if let Ok(FixerResult {
                result,
                transformed,
                fixed,
            }) = lint_fix(
                path,
                package,
                noqa,
                unsafe_fixes,
                settings,
                &source_kind,
                source_type,
            ) {
                if !fixed.is_empty() {
                    match fix_mode {
                        flags::FixMode::Apply => transformed.write(&mut File::create(path)?)?,
                        flags::FixMode::Diff => {
                            write!(
                                &mut io::stdout().lock(),
                                "{}",
                                source_kind.diff(&transformed, Some(path)).unwrap()
                            )?;
                        }
                        flags::FixMode::Generate => {}
                    }
                }
                let transformed = if let Cow::Owned(transformed) = transformed {
                    transformed
                } else {
                    source_kind
                };
                (result, transformed, fixed)
            } else {
                // If we fail to fix, lint the original source code.
                let result = lint_only(
                    path,
                    package,
                    settings,
                    noqa,
                    &source_kind,
                    source_type,
                    ParseSource::None,
                );
                let transformed = source_kind;
                let fixed = FxHashMap::default();
                (result, transformed, fixed)
            }
        } else {
            let result = lint_only(
                path,
                package,
                settings,
                noqa,
                &source_kind,
                source_type,
                ParseSource::None,
            );
            let transformed = source_kind;
            let fixed = FxHashMap::default();
            (result, transformed, fixed)
        };

    let has_error = result.has_syntax_errors();
    let messages = result.messages;

    if let Some((cache, relative_path, key)) = caching {
        // We don't cache parsing errors.
        if !has_error {
            // `FixMode::Apply` and `FixMode::Diff` rely on side-effects (writing to disk,
            // and writing the diff to stdout, respectively). If a file has diagnostics, we
            // need to avoid reading from and writing to the cache in these modes.
            if match fix_mode {
                flags::FixMode::Generate => true,
                flags::FixMode::Apply | flags::FixMode::Diff => {
                    messages.is_empty() && fixed.is_empty()
                }
            } {
                cache.update_lint(
                    relative_path.to_owned(),
                    &key,
                    LintCacheData::from_messages(
                        &messages,
                        transformed.as_ipy_notebook().map(Notebook::index).cloned(),
                    ),
                );
            }
        }
    }

    let notebook_indexes = if let SourceKind::IpyNotebook(notebook) = transformed {
        FxHashMap::from_iter([(path.to_string_lossy().to_string(), notebook.into_index())])
    } else {
        FxHashMap::default()
    };

    Ok(Diagnostics {
        messages,
        fixed: FixMap::from_iter([(fs::relativize_path(path), fixed)]),
        notebook_indexes,
    })
}

/// Generate `Diagnostic`s from source code content derived from stdin.
pub(crate) fn lint_stdin(
    path: Option<&Path>,
    package: Option<PackageRoot<'_>>,
    contents: String,
    settings: &Settings,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
) -> Result<Diagnostics> {
    let source_type = match path.and_then(|path| settings.linter.extension.get(path)) {
        None => match path.map(SourceType::from).unwrap_or_default() {
            SourceType::Python(source_type) => source_type,

            SourceType::Toml(source_type) if source_type.is_pyproject() => {
                if !settings
                    .linter
                    .rules
                    .iter_enabled()
                    .any(|rule_code| rule_code.lint_source().is_pyproject_toml())
                {
                    return Ok(Diagnostics::default());
                }

                let path = path.unwrap();
                let source_file =
                    SourceFileBuilder::new(path.to_string_lossy(), contents.clone()).finish();

                match fix_mode {
                    flags::FixMode::Diff | flags::FixMode::Generate => {}
                    flags::FixMode::Apply => write!(&mut io::stdout().lock(), "{contents}")?,
                }

                return Ok(Diagnostics {
                    messages: lint_pyproject_toml(source_file, &settings.linter),
                    fixed: FixMap::from_iter([(fs::relativize_path(path), FixTable::default())]),
                    notebook_indexes: FxHashMap::default(),
                });
            }

            SourceType::Toml(_) => return Ok(Diagnostics::default()),
        },
        Some(language) => PySourceType::from(language),
    };

    // Extract the sources from the file.
    let source_kind = match SourceKind::from_source_code(contents, source_type) {
        Ok(Some(source_kind)) => source_kind,
        Ok(None) => return Ok(Diagnostics::default()),
        Err(err) => {
            return Ok(Diagnostics::from_source_error(&err, path, &settings.linter));
        }
    };

    // Lint the inputs.
    let (LinterResult { messages, .. }, transformed, fixed) =
        if matches!(fix_mode, flags::FixMode::Apply | flags::FixMode::Diff) {
            if let Ok(FixerResult {
                result,
                transformed,
                fixed,
            }) = lint_fix(
                path.unwrap_or_else(|| Path::new("-")),
                package,
                noqa,
                settings.unsafe_fixes,
                &settings.linter,
                &source_kind,
                source_type,
            ) {
                match fix_mode {
                    flags::FixMode::Apply => {
                        // Write the contents to stdout, regardless of whether any errors were fixed.
                        transformed.write(&mut io::stdout().lock())?;
                    }
                    flags::FixMode::Diff => {
                        // But only write a diff if it's non-empty.
                        if !fixed.is_empty() {
                            write!(
                                &mut io::stdout().lock(),
                                "{}",
                                source_kind.diff(&transformed, path).unwrap()
                            )?;
                        }
                    }
                    flags::FixMode::Generate => {}
                }
                let transformed = if let Cow::Owned(transformed) = transformed {
                    transformed
                } else {
                    source_kind
                };
                (result, transformed, fixed)
            } else {
                // If we fail to fix, lint the original source code.
                let result = lint_only(
                    path.unwrap_or_else(|| Path::new("-")),
                    package,
                    &settings.linter,
                    noqa,
                    &source_kind,
                    source_type,
                    ParseSource::None,
                );

                // Write the contents to stdout anyway.
                if fix_mode.is_apply() {
                    source_kind.write(&mut io::stdout().lock())?;
                }

                let transformed = source_kind;
                let fixed = FxHashMap::default();
                (result, transformed, fixed)
            }
        } else {
            let result = lint_only(
                path.unwrap_or_else(|| Path::new("-")),
                package,
                &settings.linter,
                noqa,
                &source_kind,
                source_type,
                ParseSource::None,
            );
            let transformed = source_kind;
            let fixed = FxHashMap::default();
            (result, transformed, fixed)
        };

    let notebook_indexes = if let SourceKind::IpyNotebook(notebook) = transformed {
        FxHashMap::from_iter([(
            path.map_or_else(|| "-".into(), |path| path.to_string_lossy().to_string()),
            notebook.into_index(),
        )])
    } else {
        FxHashMap::default()
    };

    Ok(Diagnostics {
        messages,
        fixed: FixMap::from_iter([(
            fs::relativize_path(path.unwrap_or_else(|| Path::new("-"))),
            fixed,
        )]),
        notebook_indexes,
    })
}
