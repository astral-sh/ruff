#![cfg_attr(target_family = "wasm", allow(dead_code))]

use std::fs::write;
use std::io;
use std::io::Write;
use std::ops::AddAssign;
use std::path::Path;

use anyhow::{anyhow, Result};
use colored::Colorize;
use log::{debug, error};
use rustc_hash::FxHashMap;
use similar::TextDiff;

use ruff::fs;
use ruff::jupyter::{is_jupyter_notebook, JupyterIndex, JupyterNotebook};
use ruff::linter::{lint_fix, lint_only, FixTable, FixerResult, LinterResult};
use ruff::message::Message;
use ruff::settings::{flags, AllSettings, Settings};
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::SourceFileBuilder;

use crate::cache;

#[derive(Debug, Default, PartialEq)]
pub struct Diagnostics {
    pub messages: Vec<Message>,
    pub fixed: FxHashMap<String, FixTable>,
    pub imports: ImportMap,
    /// Jupyter notebook indexing table for each input file that is a jupyter notebook
    /// so we can rewrite the diagnostics in the end
    pub jupyter_index: FxHashMap<String, JupyterIndex>,
}

impl Diagnostics {
    pub fn new(messages: Vec<Message>, imports: ImportMap) -> Self {
        Self {
            messages,
            fixed: FxHashMap::default(),
            imports,
            jupyter_index: FxHashMap::default(),
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
        self.jupyter_index.extend(other.jupyter_index);
    }
}

/// Returns either an indexed python jupyter notebook or a diagnostic (which is empty if we skip)
fn load_jupyter_notebook(path: &Path) -> Result<(String, JupyterIndex), Box<Diagnostics>> {
    let notebook = match JupyterNotebook::read(path) {
        Ok(notebook) => {
            if !notebook
                .metadata
                .language_info
                .as_ref()
                .map_or(true, |language| language.name == "python")
            {
                // Not a python notebook, this could e.g. be an R notebook which we want to just skip
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
                    SourceFileBuilder::new(&path.to_string_lossy()).finish(),
                    1,
                )],
                ..Diagnostics::default()
            }));
        }
    };

    Ok(notebook.index())
}

/// Lint the source code at the given `Path`.
pub fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &AllSettings,
    cache: flags::Cache,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    // Check the cache.
    // TODO(charlie): `fixer::Mode::Apply` and `fixer::Mode::Diff` both have
    // side-effects that aren't captured in the cache. (In practice, it's fine
    // to cache `fixer::Mode::Apply`, since a file either has no fixes, or we'll
    // write the fixes to disk, thus invalidating the cache. But it's a bit hard
    // to reason about. We need to come up with a better solution here.)
    let metadata = if cache.into()
        && noqa.into()
        && matches!(autofix, flags::FixMode::None | flags::FixMode::Generate)
    {
        let metadata = path.metadata()?;
        if let Some((messages, imports)) =
            cache::get(path, package, &metadata, settings, autofix.into())
        {
            debug!("Cache hit for: {}", path.display());
            return Ok(Diagnostics::new(messages, imports));
        }
        Some(metadata)
    } else {
        None
    };

    debug!("Checking: {}", path.display());

    // Read the file from disk
    let (contents, jupyter_index) = if is_jupyter_notebook(path) {
        match load_jupyter_notebook(path) {
            Ok((contents, jupyter_index)) => (contents, Some(jupyter_index)),
            Err(diagnostics) => return Ok(*diagnostics),
        }
    } else {
        (std::fs::read_to_string(path)?, None)
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
        }) = lint_fix(&contents, path, package, noqa, &settings.lib)
        {
            if !fixed.is_empty() {
                if matches!(autofix, flags::FixMode::Apply) {
                    write(path, transformed.as_bytes())?;
                } else if matches!(autofix, flags::FixMode::Diff) {
                    let mut stdout = io::stdout().lock();
                    TextDiff::from_lines(contents.as_str(), &transformed)
                        .unified_diff()
                        .header(&fs::relativize_path(path), &fs::relativize_path(path))
                        .to_writer(&mut stdout)?;
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
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
                autofix.into(),
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
            autofix.into(),
        );
        let fixed = FxHashMap::default();
        (result, fixed)
    };

    let imports = imports.unwrap_or_default();

    if let Some(err) = parse_error {
        // Notify the user of any parse errors.
        error!(
            "{}{}{} {err}",
            "Failed to parse ".bold(),
            fs::relativize_path(path).bold(),
            ":".bold()
        );

        // Purge the cache.
        if let Some(metadata) = metadata {
            cache::del(path, package, &metadata, settings, autofix.into());
        }
    } else {
        // Re-populate the cache.
        if let Some(metadata) = metadata {
            cache::set(
                path,
                package,
                &metadata,
                settings,
                autofix.into(),
                &messages,
                &imports,
            );
        }
    }

    let jupyter_index = match jupyter_index {
        None => FxHashMap::default(),
        Some(jupyter_index) => {
            let mut index = FxHashMap::default();
            index.insert(
                path.to_str()
                    .ok_or_else(|| anyhow!("Unable to parse filename: {:?}", path))?
                    .to_string(),
                jupyter_index,
            );
            index
        }
    };

    Ok(Diagnostics {
        messages,
        fixed: FxHashMap::from_iter([(fs::relativize_path(path), fixed)]),
        imports,
        jupyter_index,
    })
}

/// Generate `Diagnostic`s from source code content derived from
/// stdin.
pub fn lint_stdin(
    path: Option<&Path>,
    package: Option<&Path>,
    contents: &str,
    settings: &Settings,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
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
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            noqa,
            settings,
        ) {
            if matches!(autofix, flags::FixMode::Apply) {
                // Write the contents to stdout, regardless of whether any errors were fixed.
                io::stdout().write_all(transformed.as_bytes())?;
            } else if matches!(autofix, flags::FixMode::Diff) {
                // But only write a diff if it's non-empty.
                if !fixed.is_empty() {
                    let text_diff = TextDiff::from_lines(contents, &transformed);
                    let mut unified_diff = text_diff.unified_diff();
                    if let Some(path) = path {
                        unified_diff.header(&fs::relativize_path(path), &fs::relativize_path(path));
                    }

                    let mut stdout = io::stdout().lock();
                    unified_diff.to_writer(&mut stdout)?;
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
                }
            }

            (result, fixed)
        } else {
            // If we fail to autofix, lint the original source code.
            let result = lint_only(
                contents,
                path.unwrap_or_else(|| Path::new("-")),
                package,
                settings,
                noqa,
                autofix.into(),
            );
            let fixed = FxHashMap::default();

            // Write the contents to stdout anyway.
            if matches!(autofix, flags::FixMode::Apply) {
                io::stdout().write_all(contents.as_bytes())?;
            }

            (result, fixed)
        }
    } else {
        let result = lint_only(
            contents,
            path.unwrap_or_else(|| Path::new("-")),
            package,
            settings,
            noqa,
            autofix.into(),
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
        jupyter_index: FxHashMap::default(),
    })
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::diagnostics::{load_jupyter_notebook, Diagnostics};

    #[test]
    fn test_r() {
        let path = Path::new("../ruff/resources/test/fixtures/jupyter/R.ipynb");
        // No diagnostics is used as skip signal
        assert_eq!(
            load_jupyter_notebook(path).unwrap_err(),
            Box::<Diagnostics>::default()
        );
    }
}
