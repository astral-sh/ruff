use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use ignore::Error;
use log::{debug, error, warn};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;

use ruff::message::{Location, Message};
use ruff::registry::Rule;
use ruff::resolver::PyprojectDiscovery;
use ruff::settings::{flags, AllSettings};
use ruff::{fs, packaging, resolver, warn_user_once, IOError, Range};
use ruff_diagnostics::Diagnostic;
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::SourceFileBuilder;

use crate::args::Overrides;
use crate::cache;
use crate::diagnostics::Diagnostics;
use crate::panic::catch_unwind;

/// Run the linter over a collection of files.
pub fn run(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    overrides: &Overrides,
    cache: flags::Cache,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    // Collect all the Python files to check.
    let start = Instant::now();
    let (paths, resolver) = resolver::python_files_in_path(files, pyproject_strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(Diagnostics::default());
    }

    // Initialize the cache.
    if cache.into() {
        fn init_cache(path: &Path) {
            if let Err(e) = cache::init(path) {
                error!("Failed to initialize cache at {}: {e:?}", path.display());
            }
        }

        match &pyproject_strategy {
            PyprojectDiscovery::Fixed(settings) => {
                init_cache(&settings.cli.cache_dir);
            }
            PyprojectDiscovery::Hierarchical(default) => {
                for settings in std::iter::once(default).chain(resolver.iter()) {
                    init_cache(&settings.cli.cache_dir);
                }
            }
        }
    };

    // Discover the package root for each Python file.
    let package_roots = packaging::detect_package_roots(
        &paths
            .iter()
            .flatten()
            .map(ignore::DirEntry::path)
            .collect::<Vec<_>>(),
        &resolver,
        pyproject_strategy,
    );

    let start = Instant::now();
    let mut diagnostics: Diagnostics = paths
        .par_iter()
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let package = path
                        .parent()
                        .and_then(|parent| package_roots.get(parent))
                        .and_then(|package| *package);
                    let settings = resolver.resolve_all(path, pyproject_strategy);

                    lint_path(path, package, settings, cache, noqa, autofix).map_err(|e| {
                        (Some(path.to_owned()), {
                            let mut error = e.to_string();
                            for cause in e.chain() {
                                error += &format!("\n  Caused by: {cause}");
                            }
                            error
                        })
                    })
                }
                Err(e) => Err((
                    if let Error::WithPath { path, .. } = e {
                        Some(path.clone())
                    } else {
                        None
                    },
                    e.io_error()
                        .map_or_else(|| e.to_string(), io::Error::to_string),
                )),
            }
            .unwrap_or_else(|(path, message)| {
                if let Some(path) = &path {
                    error!(
                        "{}{}{} {message}",
                        "Failed to lint ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    );
                    let settings = resolver.resolve(path, pyproject_strategy);
                    if settings.rules.enabled(Rule::IOError) {
                        let file = SourceFileBuilder::new(&path.to_string_lossy()).finish();

                        Diagnostics::new(
                            vec![Message::from_diagnostic(
                                Diagnostic::new(
                                    IOError { message },
                                    Range::new(Location::default(), Location::default()),
                                ),
                                file,
                                1,
                            )],
                            ImportMap::default(),
                        )
                    } else {
                        Diagnostics::default()
                    }
                } else {
                    error!("{} {message}", "Encountered error:".bold());
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
    debug!("Checked {:?} files in: {:?}", paths.len(), duration);

    Ok(diagnostics)
}

/// Wraps [`lint_path`](crate::diagnostics::lint_path) in a [`catch_unwind`](std::panic::catch_unwind) and emits
/// a diagnostic if the linting the file panics.
fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &AllSettings,
    cache: flags::Cache,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    let result = catch_unwind(|| {
        crate::diagnostics::lint_path(path, package, settings, cache, noqa, autofix)
    });

    match result {
        Ok(inner) => inner,
        Err(error) => {
            let message = r#"This indicates a bug in `ruff`. If you could open an issue at:

https://github.com/charliermarsh/ruff/issues/new?title=%5BLinter%20panic%5D

with the relevant file contents, the `pyproject.toml` settings, and the following stack trace, we'd be very appreciative!
"#;

            warn!(
                "{}{}{} {message}\n{error}",
                "Linting panicked ".bold(),
                fs::relativize_path(path).bold(),
                ":".bold()
            );

            Ok(Diagnostics::default())
        }
    }
}

#[cfg(test)]
#[cfg(feature = "jupyter_notebook")]
mod test {
    use std::path::PathBuf;
    use std::str::FromStr;

    use anyhow::Result;
    use path_absolutize::Absolutize;

    use ruff::logging::LogLevel;
    use ruff::resolver::PyprojectDiscovery;
    use ruff::settings::configuration::{Configuration, RuleSelection};
    use ruff::settings::flags::FixMode;
    use ruff::settings::flags::{Cache, Noqa};
    use ruff::settings::types::SerializationFormat;
    use ruff::settings::AllSettings;
    use ruff::RuleSelector;

    use crate::args::Overrides;
    use crate::printer::{Flags, Printer};

    use super::run;

    #[test]
    fn test_jupyter_notebook_integration() -> Result<()> {
        let overrides: Overrides = Overrides {
            select: Some(vec![
                RuleSelector::from_str("B")?,
                RuleSelector::from_str("F")?,
            ]),
            ..Default::default()
        };

        let mut configuration = Configuration::default();
        configuration.rule_selections.push(RuleSelection {
            select: Some(vec![
                RuleSelector::from_str("B")?,
                RuleSelector::from_str("F")?,
            ]),
            ..Default::default()
        });

        let root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("ruff")
            .join("resources")
            .join("test")
            .join("fixtures")
            .join("jupyter");

        let diagnostics = run(
            &[root_path.join("valid.ipynb")],
            &PyprojectDiscovery::Fixed(AllSettings::from_configuration(configuration, &root_path)?),
            &overrides,
            Cache::Disabled,
            Noqa::Enabled,
            FixMode::None,
        )?;

        let printer = Printer::new(
            SerializationFormat::Text,
            LogLevel::Default,
            FixMode::None,
            Flags::SHOW_VIOLATIONS,
        );
        let mut writer: Vec<u8> = Vec::new();
        // Mute the terminal color codes
        colored::control::set_override(false);
        printer.write_once(&diagnostics, &mut writer)?;
        // TODO(konstin): Set jupyter notebooks as none-fixable for now
        // TODO(konstin) 2: Make jupyter notebooks fixable
        let expected = format!(
            "{valid_ipynb}:cell 1:2:5: F841 [*] Local variable `x` is assigned to but never used
{valid_ipynb}:cell 3:1:24: B006 Do not use mutable data structures for argument defaults
Found 2 errors.
[*] 1 potentially fixable with the --fix option.
",
            valid_ipynb = root_path.join("valid.ipynb").absolutize()?.display()
        );

        assert_eq!(expected, String::from_utf8(writer)?);

        Ok(())
    }
}
