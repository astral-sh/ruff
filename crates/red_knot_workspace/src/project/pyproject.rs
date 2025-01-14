mod package_name;

use pep440_rs::{Version, VersionSpecifiers};
use serde::Deserialize;
use thiserror::Error;

pub(crate) use package_name::PackageName;

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PyProject {
    /// PEP 621-compliant project metadata.
    pub project: Option<Project>,
    /// Tool-specific metadata.
    pub tool: Option<Tool>,
}

impl PyProject {
    pub(crate) fn knot(&self) -> Option<&Knot> {
        self.tool.as_ref().and_then(|tool| tool.knot.as_ref())
    }
}

#[derive(Error, Debug)]
pub enum PyProjectError {
    #[error(transparent)]
    TomlSyntax(#[from] toml::de::Error),
}

impl PyProject {
    pub(crate) fn from_str(content: &str) -> Result<Self, PyProjectError> {
        toml::from_str(content).map_err(PyProjectError::TomlSyntax)
    }
}

/// PEP 621 project metadata (`project`).
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(serde::Serialize))]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Project {
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

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tool {
    pub knot: Option<Knot>,
}

// TODO(micha): Remove allow once we add knot settings.
//  We can't use a unit struct here or deserializing `[tool.knot]` fails.
#[allow(clippy::empty_structs_with_brackets)]
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct Knot {}
