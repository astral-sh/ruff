use crate::metadata::options::Options;
use crate::metadata::value::{RangedValue, ValueSource, ValueSourceGuard};
use pep440_rs::{release_specifiers_to_ranges, Version, VersionSpecifiers};
use ruff_python_ast::PythonVersion;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::Bound;
use std::ops::Deref;
use thiserror::Error;

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
    pub(crate) fn ty(&self) -> Option<&Options> {
        self.tool.as_ref().and_then(|tool| tool.ty.as_ref())
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
        let _guard = ValueSourceGuard::new(source, true);
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
    pub name: Option<RangedValue<PackageName>>,
    /// The version of the project
    pub version: Option<RangedValue<Version>>,
    /// The Python versions this project is compatible with.
    pub requires_python: Option<RangedValue<VersionSpecifiers>>,
}

impl Project {
    pub(super) fn resolve_requires_python_lower_bound(
        &self,
    ) -> Result<Option<RangedValue<PythonVersion>>, ResolveRequiresPythonError> {
        let Some(requires_python) = self.requires_python.as_ref() else {
            return Ok(None);
        };

        tracing::debug!("Resolving requires-python constraint: `{requires_python}`");

        let ranges = release_specifiers_to_ranges((**requires_python).clone());
        let Some((lower, _)) = ranges.bounding_range() else {
            return Ok(None);
        };

        let version = match lower {
            // Ex) `>=3.10.1` -> `>=3.10`
            Bound::Included(version) => version,

            // Ex) `>3.10.1` -> `>=3.10` or `>3.10` -> `>=3.10`
            // The second example looks obscure at first but it is required because
            // `3.10.1 > 3.10` is true but we only have two digits here. So including 3.10 is the
            // right move. Overall, using `>` without a patch release is most likely bogus.
            Bound::Excluded(version) => version,

            // Ex) `<3.10` or ``
            Bound::Unbounded => {
                return Err(ResolveRequiresPythonError::NoLowerBound(
                    requires_python.to_string(),
                ))
            }
        };

        // Take the major and minor version
        let mut versions = version.release().iter().take(2);

        let Some(major) = versions.next().copied() else {
            return Ok(None);
        };

        let minor = versions.next().copied().unwrap_or_default();

        tracing::debug!("Resolved requires-python constraint to: {major}.{minor}");

        let major =
            u8::try_from(major).map_err(|_| ResolveRequiresPythonError::TooLargeMajor(major))?;
        let minor =
            u8::try_from(minor).map_err(|_| ResolveRequiresPythonError::TooLargeMinor(minor))?;

        Ok(Some(
            requires_python
                .clone()
                .map_value(|_| PythonVersion::from((major, minor))),
        ))
    }
}

#[derive(Debug, Error)]
pub enum ResolveRequiresPythonError {
    #[error("The major version `{0}` is larger than the maximum supported value 255")]
    TooLargeMajor(u64),
    #[error("The minor version `{0}` is larger than the maximum supported value 255")]
    TooLargeMinor(u64),
    #[error("value `{0}` does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `>=3.13`) or specify a version in `environment.python-version`.")]
    NoLowerBound(String),
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Tool {
    pub ty: Option<Options>,
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
