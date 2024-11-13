mod package_name;

use pep440_rs::{Version, VersionSpecifiers};
use serde::Deserialize;
use thiserror::Error;

use crate::workspace::metadata::WorkspaceDiscoveryError;
pub(crate) use package_name::PackageName;
use ruff_db::system::SystemPath;

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
    pub(crate) fn workspace(&self) -> Option<&Workspace> {
        self.tool
            .as_ref()
            .and_then(|tool| tool.knot.as_ref())
            .and_then(|knot| knot.workspace.as_ref())
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

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct Knot {
    pub(crate) workspace: Option<Workspace>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct Workspace {
    pub(crate) members: Option<Vec<String>>,
    pub(crate) exclude: Option<Vec<String>>,
}

impl Workspace {
    pub(crate) fn members(&self) -> &[String] {
        self.members.as_deref().unwrap_or_default()
    }

    pub(crate) fn exclude(&self) -> &[String] {
        self.exclude.as_deref().unwrap_or_default()
    }

    pub(crate) fn is_excluded(
        &self,
        path: &SystemPath,
        workspace_root: &SystemPath,
    ) -> Result<bool, WorkspaceDiscoveryError> {
        // If there's an explicit include, then that wins.
        if self
            .members()
            .iter()
            .any(|member| workspace_root.join(member).starts_with(path))
        {
            return Ok(false);
        }

        for exclude in self.exclude() {
            let full_glob =
                glob::Pattern::new(workspace_root.join(exclude).as_str()).map_err(|error| {
                    WorkspaceDiscoveryError::InvalidMembersPattern {
                        raw_glob: exclude.clone(),
                        source: error,
                    }
                })?;

            if full_glob.matches_path(path.as_std_path()) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
