use pep440_rs::{Version, VersionSpecifiers};
use serde::{Deserialize, Deserializer, Serialize};
use std::ops::Deref;
use thiserror::Error;

use crate::metadata::options::Options;
use crate::metadata::value::{ValueSource, ValueSourceGuard};

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PyProject {
    /// PEP 621-compliant project metadata.
    pub project: Option<Project>,
    /// Tool-specific metadata.
    pub tool: Option<Tool>,
}

impl PyProject {
    pub(crate) fn knot(&self) -> Option<&Options> {
        self.tool.as_ref().and_then(|tool| tool.knot.as_ref())
    }
}

#[derive(Error, Debug)]
pub enum PyProjectError {
    #[error(transparent)]
    TomlSyntax(#[from] toml::de::Error),
}

impl PyProject {
    pub(crate) fn from_toml_str(
        content: &str,
        source: ValueSource,
    ) -> Result<Self, PyProjectError> {
        let _guard = ValueSourceGuard::new(source);
        toml::from_str(content).map_err(PyProjectError::TomlSyntax)
    }
}

/// PEP 621 project metadata (`project`).
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    /// The name of the project
    ///
    /// Note: Intentionally option to be more permissive during deserialization.
    /// `PackageMetadata::from_pyproject` reports missing names.
    pub name: Option<PackageName>,
    /// The version of the project
    pub version: Option<Version>,
    /// The Python versions this project is compatible with.
    pub requires_python: Option<VersionSpecifiers>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Tool {
    pub knot: Option<Options>,
}

/// The normalized name of a package.
///
/// Converts the name to lowercase and collapses runs of `-`, `_`, and `.` down to a single `-`.
/// For example, `---`, `.`, and `__` are all converted to a single `-`.
///
/// See: <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct PackageName(String);

impl PackageName {
    /// Create a validated, normalized package name.
    pub(crate) fn new(name: String) -> Result<Self, InvalidPackageNameError> {
        if name.is_empty() {
            return Err(InvalidPackageNameError::Empty);
        }

        if name.starts_with(['-', '_', '.']) {
            return Err(InvalidPackageNameError::NonAlphanumericStart(
                name.chars().next().unwrap(),
            ));
        }

        if name.ends_with(['-', '_', '.']) {
            return Err(InvalidPackageNameError::NonAlphanumericEnd(
                name.chars().last().unwrap(),
            ));
        }

        let Some(start) = name.find(|c: char| {
            !c.is_ascii() || c.is_ascii_uppercase() || matches!(c, '-' | '_' | '.')
        }) else {
            return Ok(Self(name));
        };

        let (already_normalized, maybe_normalized) = name.split_at(start);

        let mut normalized = String::with_capacity(name.len());
        normalized.push_str(already_normalized);
        let mut last = None;

        for c in maybe_normalized.chars() {
            if !c.is_ascii() {
                return Err(InvalidPackageNameError::InvalidCharacter(c));
            }

            if c.is_ascii_uppercase() {
                normalized.push(c.to_ascii_lowercase());
            } else if matches!(c, '-' | '_' | '.') {
                if matches!(last, Some('-' | '_' | '.')) {
                    // Only keep a single instance of `-`, `_` and `.`
                } else {
                    normalized.push('-');
                }
            } else {
                normalized.push(c);
            }

            last = Some(c);
        }

        Ok(Self(normalized))
    }

    /// Returns the underlying package name.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<PackageName> for String {
    fn from(value: PackageName) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for PackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for PackageName {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

#[derive(Error, Debug)]
pub(crate) enum InvalidPackageNameError {
    #[error("name must start with letter or number but it starts with '{0}'")]
    NonAlphanumericStart(char),
    #[error("name must end with letter or number but it ends with '{0}'")]
    NonAlphanumericEnd(char),
    #[error("valid name consists only of ASCII letters and numbers, period, underscore and hyphen but name contains '{0}'"
    )]
    InvalidCharacter(char),
    #[error("name must not be empty")]
    Empty,
}

#[cfg(test)]
mod tests {
    use super::PackageName;

    #[test]
    fn normalize() {
        let inputs = [
            "friendly-bard",
            "Friendly-Bard",
            "FRIENDLY-BARD",
            "friendly.bard",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];

        for input in inputs {
            assert_eq!(
                PackageName::new(input.to_string()).unwrap(),
                PackageName::new("friendly-bard".to_string()).unwrap(),
            );
        }
    }
}
