//! Utilities for locating (and extracting configuration from) a pyproject.toml.

use std::path::{Path, PathBuf};

use crate::options::Options;
use anyhow::Result;
use log::debug;
use pep440_rs::VersionSpecifiers;
use ruff::settings::types::PythonVersion;
use serde::{Deserialize, Serialize};

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
    let contents = std::fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(Into::into)
}

/// Parse a `pyproject.toml` file.
fn parse_pyproject_toml<P: AsRef<Path>>(path: P) -> Result<Pyproject> {
    let contents = std::fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(Into::into)
}

/// Return `true` if a `pyproject.toml` contains a `[tool.ruff]` section.
pub fn ruff_enabled<P: AsRef<Path>>(path: P) -> Result<bool> {
    let pyproject = parse_pyproject_toml(path)?;
    Ok(pyproject.tool.and_then(|tool| tool.ruff).is_some())
}

/// Return the path to the `pyproject.toml` or `ruff.toml` file in a given
/// directory.
pub fn settings_toml<P: AsRef<Path>>(path: P) -> Result<Option<PathBuf>> {
    // Check for `.ruff.toml`.
    let ruff_toml = path.as_ref().join(".ruff.toml");
    if ruff_toml.is_file() {
        return Ok(Some(ruff_toml));
    }

    // Check for `ruff.toml`.
    let ruff_toml = path.as_ref().join("ruff.toml");
    if ruff_toml.is_file() {
        return Ok(Some(ruff_toml));
    }

    // Check for `pyproject.toml`.
    let pyproject_toml = path.as_ref().join("pyproject.toml");
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

/// Find the path to the user-specific `pyproject.toml` or `ruff.toml`, if it
/// exists.
pub fn find_user_settings_toml() -> Option<PathBuf> {
    // Search for a user-specific `.ruff.toml`.
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push(".ruff.toml");
    if path.is_file() {
        return Some(path);
    }

    // Search for a user-specific `ruff.toml`.
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push("ruff.toml");
    if path.is_file() {
        return Some(path);
    }

    // Search for a user-specific `pyproject.toml`.
    let mut path = dirs::config_dir()?;
    path.push("ruff");
    path.push("pyproject.toml");
    if path.is_file() {
        return Some(path);
    }

    None
}

/// Load `Options` from a `pyproject.toml` or `ruff.toml` file.
pub fn load_options<P: AsRef<Path>>(path: P) -> Result<Options> {
    if path.as_ref().ends_with("pyproject.toml") {
        let pyproject = parse_pyproject_toml(&path)?;
        let mut ruff = pyproject
            .tool
            .and_then(|tool| tool.ruff)
            .unwrap_or_default();
        if ruff.target_version.is_none() {
            if let Some(project) = pyproject.project {
                if let Some(requires_python) = project.requires_python {
                    ruff.target_version =
                        PythonVersion::get_minimum_supported_version(&requires_python);
                }
            }
        }
        Ok(ruff)
    } else {
        let ruff = parse_ruff_toml(path);
        if let Ok(ruff) = &ruff {
            if ruff.target_version.is_none() {
                debug!("`project.requires_python` in `pyproject.toml` will not be used to set `target_version` when using `ruff.toml`.");
            }
        }
        ruff
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::options::Options;
    use crate::pyproject::{find_settings_toml, parse_pyproject_toml, Pyproject, Tools};
    use crate::tests::test_resource_path;
    use anyhow::Result;
    use ruff::codes;
    use ruff::line_width::LineLength;
    use ruff::settings::types::PatternPrefixPair;
    use rustc_hash::FxHashMap;

    #[test]

    fn deserialize() -> Result<()> {
        let pyproject: Pyproject = toml::from_str(r#""#)?;
        assert_eq!(pyproject.tool, None);

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
"#,
        )?;
        assert_eq!(pyproject.tool, Some(Tools { ruff: None }));

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options::default())
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
line-length = 79
"#,
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
[tool.ruff]
select = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    select: Some(vec![codes::Pycodestyle::E501.into()]),
                    ..Options::default()
                })
            })
        );

        let pyproject: Pyproject = toml::from_str(
            r#"
[tool.black]
[tool.ruff]
extend-select = ["RUF100"]
ignore = ["E501"]
"#,
        )?;
        assert_eq!(
            pyproject.tool,
            Some(Tools {
                ruff: Some(Options {
                    extend_select: Some(vec![codes::Ruff::_100.into()]),
                    ignore: Some(vec![codes::Pycodestyle::E501.into()]),
                    ..Options::default()
                })
            })
        );

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff]
line_length = 79
"#,
        )
        .is_err());

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff]
select = ["E123"]
"#,
        )
        .is_err());

        assert!(toml::from_str::<Pyproject>(
            r#"
[tool.black]
[tool.ruff]
line-length = 79
other-attribute = 1
"#,
        )
        .is_err());

        Ok(())
    }

    #[test]
    fn find_and_parse_pyproject_toml() -> Result<()> {
        let pyproject = find_settings_toml(test_resource_path("fixtures/__init__.py"))?.unwrap();
        assert_eq!(pyproject, test_resource_path("fixtures/pyproject.toml"));

        let pyproject = parse_pyproject_toml(&pyproject)?;
        let config = pyproject.tool.unwrap().ruff.unwrap();
        assert_eq!(
            config,
            Options {
                line_length: Some(LineLength::try_from(88).unwrap()),
                extend_exclude: Some(vec![
                    "excluded_file.py".to_string(),
                    "migrations".to_string(),
                    "with_excluded_file/other_excluded_file.py".to_string(),
                ]),
                per_file_ignores: Some(FxHashMap::from_iter([(
                    "__init__.py".to_string(),
                    vec![codes::Pyflakes::_401.into()]
                )])),
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
        let result = PatternPrefixPair::from_str("bar:E502");
        assert!(result.is_err());
    }
}
