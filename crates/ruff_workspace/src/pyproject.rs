//! Utilities for locating (and extracting configuration from) a pyproject.toml.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::debug;
use pep440_rs::{Operator, Version, VersionSpecifiers};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use ruff_linter::settings::types::PythonVersion;

use crate::options::Options;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Tools {
    ruff: Option<Options>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
struct Project {
    #[serde(alias = "requires-python", alias = "requires_python")]
    requires_python: Option<VersionSpecifiers>,
}

#[derive(Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Pyproject {
    tool: Option<Tools>,
    project: Option<Project>,
}

impl Pyproject {
    pub const fn new(options: Options) -> Self {
        Self {
            tool: Some(Tools {
                ruff: Some(options),
            }),
            project: None,
        }
    }
}

/// Parse a `ruff.toml` file.
fn parse_ruff_toml<P: AsRef<Path>>(path: P) -> Result<Options> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))
}

/// Parse a `pyproject.toml` file.
fn parse_pyproject_toml<P: AsRef<Path>>(path: P) -> Result<Pyproject> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))
}

/// Return `true` if a `pyproject.toml` contains a `[tool.ruff]` section.
pub fn ruff_enabled<P: AsRef<Path>>(path: P) -> Result<bool> {
    let pyproject = parse_pyproject_toml(path)?;
    Ok(pyproject.tool.and_then(|tool| tool.ruff).is_some())
}

/// Return the path to the `pyproject.toml` or `ruff.toml` file in a given
/// directory.
pub fn settings_toml<P: AsRef<Path>>(path: P) -> Result<Option<PathBuf>> {
    let path = path.as_ref();
    // Check for `.ruff.toml`.
    let ruff_toml = path.join(".ruff.toml");
    if ruff_toml.is_file() {
        return Ok(Some(ruff_toml));
    }

    // Check for `ruff.toml`.
    let ruff_toml = path.join("ruff.toml");
    if ruff_toml.is_file() {
        return Ok(Some(ruff_toml));
    }

    // Check for `pyproject.toml`.
    let pyproject_toml = path.join("pyproject.toml");
    if pyproject_toml.is_file() && ruff_enabled(&pyproject_toml)? {
        return Ok(Some(pyproject_toml));
    }

    Ok(None)
}

/// Find the path to the `pyproject.toml` or `ruff.toml` file, if such a file
/// exists.
pub fn find_settings_toml<P: AsRef<Path>>(path: P) -> Result<Option<PathBuf>> {
    for directory in path.as_ref().ancestors() {
        if let Some(pyproject) = settings_toml(directory)? {
            return Ok(Some(pyproject));
        }
    }
    Ok(None)
}

/// Derive target version from `required-version` in `pyproject.toml`, if
/// such a file exists in an ancestor directory.
pub fn find_fallback_target_version<P: AsRef<Path>>(path: P) -> Option<PythonVersion> {
    for directory in path.as_ref().ancestors() {
        if let Some(fallback) = get_fallback_target_version(directory) {
            return Some(fallback);
        }
    }
    None
}

/// Find the path to the user-specific `pyproject.toml` or `ruff.toml`, if it
/// exists.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_user_settings_toml() -> Option<PathBuf> {
    use etcetera::BaseStrategy;
    use ruff_linter::warn_user_once;

    let strategy = etcetera::base_strategy::choose_base_strategy().ok()?;
    let config_dir = strategy.config_dir().join("ruff");

    // Search for a user-specific `.ruff.toml`, then a `ruff.toml`, then a `pyproject.toml`.
    for filename in [".ruff.toml", "ruff.toml", "pyproject.toml"] {
        let path = config_dir.join(filename);
        if path.is_file() {
            return Some(path);
        }
    }

    // On macOS, we used to support reading from `/Users/Alice/Library/Application Support`.
    if cfg!(target_os = "macos") {
        let strategy = etcetera::base_strategy::Apple::new().ok()?;
        let deprecated_config_dir = strategy.data_dir().join("ruff");

        for file in [".ruff.toml", "ruff.toml", "pyproject.toml"] {
            let path = deprecated_config_dir.join(file);
            if path.is_file() {
                warn_user_once!(
                    "Reading configuration from `~/Library/Application Support` is deprecated. Please move your configuration to `{}/{file}`.",
                    config_dir.display(),
                );
                return Some(path);
            }
        }
    }

    None
}

#[cfg(target_arch = "wasm32")]
pub fn find_user_settings_toml() -> Option<PathBuf> {
    None
}

/// Load `Options` from a `pyproject.toml` or `ruff.toml` file.
pub(super) fn load_options<P: AsRef<Path>>(
    path: P,
    version_strategy: &TargetVersionStrategy,
) -> Result<Options> {
    let path = path.as_ref();
    if path.ends_with("pyproject.toml") {
        let pyproject = parse_pyproject_toml(path)?;
        let mut ruff = pyproject
            .tool
            .and_then(|tool| tool.ruff)
            .unwrap_or_default();
        if ruff.target_version.is_none() {
            if let Some(project) = pyproject.project {
                if let Some(requires_python) = project.requires_python {
                    ruff.target_version = get_minimum_supported_version(&requires_python);
                }
            }
        }
        Ok(ruff)
    } else {
        let mut ruff = parse_ruff_toml(path);
        if let Ok(ref mut ruff) = ruff {
            if ruff.target_version.is_none() {
                debug!("No `target-version` found in `ruff.toml`");
                match version_strategy {
                    TargetVersionStrategy::UseDefault => {}
                    TargetVersionStrategy::RequiresPythonFallback => {
                        if let Some(dir) = path.parent() {
                            let fallback = get_fallback_target_version(dir);
                            if let Some(version) = fallback {
                                debug!("Derived `target-version` from `requires-python` in `pyproject.toml`: {version:?}");
                            } else {
                                debug!("No `pyproject.toml` with `requires-python` in same directory; `target-version` unspecified");
                            }
                            ruff.target_version = fallback;
                        }
                    }
                }
            }
        }
        ruff
    }
}

/// Extract `target-version` from `pyproject.toml` in the given directory
/// if the file exists and has `requires-python`.
fn get_fallback_target_version(dir: &Path) -> Option<PythonVersion> {
    let pyproject_path = dir.join("pyproject.toml");
    if !pyproject_path.exists() {
        return None;
    }
    let parsed_pyproject = parse_pyproject_toml(&pyproject_path);

    let pyproject = match parsed_pyproject {
        Ok(pyproject) => pyproject,
        Err(err) => {
            debug!("Failed to find fallback `target-version` due to: {err}");
            return None;
        }
    };

    if let Some(project) = pyproject.project {
        if let Some(requires_python) = project.requires_python {
            return get_minimum_supported_version(&requires_python);
        }
    }
    None
}

/// Infer the minimum supported [`PythonVersion`] from a `requires-python` specifier.
fn get_minimum_supported_version(requires_version: &VersionSpecifiers) -> Option<PythonVersion> {
    /// Truncate a version to its major and minor components.
    fn major_minor(version: &Version) -> Option<Version> {
        let major = version.release().first()?;
        let minor = version.release().get(1)?;
        Some(Version::new([major, minor]))
    }

    // Extract the minimum supported version from the specifiers.
    let minimum_version = requires_version
        .iter()
        .filter(|specifier| {
            matches!(
                specifier.operator(),
                Operator::Equal
                    | Operator::EqualStar
                    | Operator::ExactEqual
                    | Operator::TildeEqual
                    | Operator::GreaterThan
                    | Operator::GreaterThanEqual
            )
        })
        .filter_map(|specifier| major_minor(specifier.version()))
        .min()?;

    debug!("Detected minimum supported `requires-python` version: {minimum_version}");

    // Find the Python version that matches the minimum supported version.
    PythonVersion::iter().find(|version| Version::from(*version) == minimum_version)
}

/// Strategy for handling missing `target-version` in configuration.
#[derive(Debug)]
pub(super) enum TargetVersionStrategy {
    /// Use default `target-version`
    UseDefault,
    /// Derive from `requires-python` if available
    RequiresPythonFallback,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::str::FromStr;

    use anyhow::{Context, Result};
    use rustc_hash::FxHashMap;
    use tempfile::TempDir;

    use ruff_linter::codes;
    use ruff_linter::line_width::LineLength;
    use ruff_linter::settings::types::PatternPrefixPair;

    use crate::options::{Flake8BuiltinsOptions, LintCommonOptions, LintOptions, Options};
    use crate::pyproject::{find_settings_toml, parse_pyproject_toml, Pyproject, Tools};

    #[test]

    fn deserialize() -> Result<()> {
        let pyproject: Pyproject = toml::from_str(r"")?;
        assert_eq!(pyproject.tool, None);

        let pyproject: Pyproject = toml::from_str(
            r"
[tool.black]
",
        )?;
        assert_eq!(pyproject.tool, Some(Tools { ruff: None }));

        let pyproject: Pyproject = toml::from_str(
            r"
[tool.black]
[tool.ruff]
",
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options::default())
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r"
[tool.black]
[tool.ruff]
line-length = 79
",
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    line_length: Some(LineLength::try_from(79).unwrap()),
                    ..Options::default()
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
exclude = ["foo.py"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    exclude: Some(vec!["foo.py".to_string()]),
                    ..Options::default()
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff.lint]
select = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    lint: Some(LintOptions {
                        common: LintCommonOptions {
                            select: Some(vec![codes::Pycodestyle::E501.into()]),
                            ..LintCommonOptions::default()
                        },
                        ..LintOptions::default()
                    }),
                    ..Options::default()
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff.lint]
extend-select = ["RUF100"]
ignore = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    lint: Some(LintOptions {
                        common: LintCommonOptions {
                            extend_select: Some(vec![codes::Ruff::_100.into()]),
                            ignore: Some(vec![codes::Pycodestyle::E501.into()]),
                            ..LintCommonOptions::default()
                        },
                        ..LintOptions::default()
                    }),
                    ..Options::default()
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.ruff.lint.flake8-builtins]
builtins-allowed-modules = ["asyncio"]
builtins-ignorelist = ["argparse", 'typing']
builtins-strict-checking = true
allowed-modules = ['sys']
ignorelist = ["os", 'io']
strict-checking = false
"#,
        )?;

        #[allow(deprecated)]
        let expected = Flake8BuiltinsOptions {
            builtins_allowed_modules: Some(vec!["asyncio".to_string()]),
            allowed_modules: Some(vec!["sys".to_string()]),

            builtins_ignorelist: Some(vec!["argparse".to_string(), "typing".to_string()]),
            ignorelist: Some(vec!["os".to_string(), "io".to_string()]),

            builtins_strict_checking: Some(true),
            strict_checking: Some(false),
        };

        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    lint: Some(LintOptions {
                        common: LintCommonOptions {
                            flake8_builtins: Some(expected.clone()),
                            ..LintCommonOptions::default()
                        },
                        ..LintOptions::default()
                    }),
                    ..Options::default()
                })
            })
        );

        let settings = expected.into_settings();

        assert_eq!(settings.allowed_modules, vec!["sys".to_string()]);
        assert_eq!(
            settings.ignorelist,
            vec!["os".to_string(), "io".to_string()]
        );
        assert!(!settings.strict_checking);

        assert!(toml::from_str::<Pyproject>(
            r"
[tool.black]
[tool.ruff]
line_length = 79
",
        )
        .is_err());

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff.lint]
select = ["E123"]
"#,
        )
        .is_err());

        assert!(toml::from_str::<Pyproject>(
            r"
[tool.black]
[tool.ruff]
line-length = 79
other-attribute = 1
",
        )
        .is_err());

        Ok(())
    }

    #[test]
    fn find_and_parse_pyproject_toml() -> Result<()> {
        let tempdir = TempDir::new()?;
        let ruff_toml = tempdir.path().join("pyproject.toml");
        fs::write(
            ruff_toml,
            r#"
[tool.ruff]
line-length = 88
extend-exclude = [
  "excluded_file.py",
  "migrations",
  "with_excluded_file/other_excluded_file.py",
]

[tool.ruff.lint]
per-file-ignores = { "__init__.py" = ["F401"] }
"#,
        )?;

        let pyproject =
            find_settings_toml(tempdir.path())?.context("Failed to find pyproject.toml")?;
        let pyproject = parse_pyproject_toml(pyproject)?;
        let config = pyproject
            .tool
            .context("Expected to find [tool] field")?
            .ruff
            .context("Expected to find [tool.ruff] field")?;
        assert_eq!(
            config,
            Options {
                line_length: Some(LineLength::try_from(88).unwrap()),
                extend_exclude: Some(vec![
                    "excluded_file.py".to_string(),
                    "migrations".to_string(),
                    "with_excluded_file/other_excluded_file.py".to_string(),
                ]),

                lint: Some(LintOptions {
                    common: LintCommonOptions {
                        per_file_ignores: Some(FxHashMap::from_iter([(
                            "__init__.py".to_string(),
                            vec![codes::Pyflakes::_401.into()]
                        )])),
                        ..LintCommonOptions::default()
                    },
                    ..LintOptions::default()
                }),
                ..Options::default()
            }
        );

        Ok(())
    }

    #[test]
    fn str_pattern_prefix_pair() {
        let result = PatternPrefixPair::from_str("foo:E501");
        assert!(result.is_ok());
        let result = PatternPrefixPair::from_str("foo: E501");
        assert!(result.is_ok());
        let result = PatternPrefixPair::from_str("E501:foo");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("E501");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("foo");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("foo:E501:E402");
        assert!(result.is_err());
        let result = PatternPrefixPair::from_str("**/bar:E501");
        assert!(result.is_ok());
        let result = PatternPrefixPair::from_str("bar:E503");
        assert!(result.is_err());
    }
}
