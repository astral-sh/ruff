use std::collections::BTreeMap;
use std::path::PathBuf;

use compact_str::CompactString;
use pep440_rs::Version;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_ranged_value::{RangedValue, ValueSource};
use serde::Deserialize;
use thiserror::Error;

use crate::metadata::python_version::SupportedPythonVersion;

mod dependencies;

pub(crate) use dependencies::DependencyMetadataError;

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct UvMetadata {
    workspace_root: SystemPathBuf,
    members: Box<[WorkspaceMember]>,
    environment: Option<SystemPathBuf>,
    python_version: Option<RangedValue<SupportedPythonVersion>>,
    schema: Schema,
    workspace: Option<NodeReference>,
    resolution: BTreeMap<CompactString, ResolutionNode>,
    module_owners: BTreeMap<CompactString, Box<[ModuleOwner]>>,
}

impl UvMetadata {
    pub(crate) fn workspace_root(&self) -> &SystemPath {
        &self.workspace_root
    }

    /// Workspace members returned by uv. Empty for standalone scripts.
    #[cfg(test)]
    pub(crate) fn members(&self) -> &[WorkspaceMember] {
        &self.members
    }

    pub(crate) fn environment(&self) -> Option<&SystemPath> {
        self.environment.as_deref()
    }

    pub(crate) fn python_version(&self) -> Option<&RangedValue<SupportedPythonVersion>> {
        self.python_version.as_ref()
    }

    pub(crate) fn from_metadata(
        metadata: &[u8],
        system: &dyn System,
    ) -> Result<Self, UvMetadataError> {
        let metadata = serde_json::from_slice::<WorkspaceMetadata>(metadata)
            .map_err(UvMetadataError::InvalidMetadata)?;

        let workspace_root = existing_directory(metadata.workspace_root, "workspace root", system)?;

        let (environment, python_version) = match metadata.environment {
            Some(environment) => (
                Some(existing_directory(
                    environment.root,
                    "environment root",
                    system,
                )?),
                Some(resolve_python_version(&environment.python.version)?),
            ),
            None => (None, None),
        };

        Ok(Self {
            workspace_root,
            members: metadata.members,
            environment,
            python_version,
            schema: metadata.schema,
            workspace: metadata.workspace,
            resolution: metadata.resolution,
            module_owners: metadata.module_owners,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
pub(crate) struct WorkspaceMember {
    pub(crate) name: Box<str>,
    /// Directory containing the member's `pyproject.toml`.
    pub(crate) path: SystemPathBuf,
    id: CompactString,
}

#[derive(Debug, Error)]
pub(crate) enum UvMetadataError {
    #[error("Failed to invoke `uv workspace metadata`: {0}")]
    Invocation(#[source] std::io::Error),

    #[error("`uv workspace metadata` failed with status {status}: {stderr}")]
    CommandFailed {
        status: std::process::ExitStatus,
        stderr: String,
    },

    #[error("invalid `uv workspace metadata` JSON: {0}")]
    InvalidMetadata(serde_json::Error),

    #[error("unsupported Python version `{0}` returned by `uv workspace metadata`")]
    InvalidPythonVersion(Version),

    #[error("non-Unicode {description} returned by `uv workspace metadata`: `{path}`", path = path.display())]
    NonUnicodePath {
        description: &'static str,
        path: PathBuf,
    },

    #[error("missing {description} returned by `uv workspace metadata`: `{path}`")]
    MissingDirectory {
        description: &'static str,
        path: SystemPathBuf,
    },
}
fn existing_directory(
    path: PathBuf,
    description: &'static str,
    system: &dyn System,
) -> Result<SystemPathBuf, UvMetadataError> {
    let path = match SystemPathBuf::from_path_buf(path) {
        Ok(path) => path,
        Err(path) => return Err(UvMetadataError::NonUnicodePath { description, path }),
    };

    if !system.is_directory(&path) {
        return Err(UvMetadataError::MissingDirectory { description, path });
    }

    Ok(path)
}

fn resolve_python_version(
    version: &Version,
) -> Result<RangedValue<SupportedPythonVersion>, UvMetadataError> {
    let [major, minor, ..] = version.release() else {
        return Err(UvMetadataError::InvalidPythonVersion(version.clone()));
    };
    let version = format!("{major}.{minor}")
        .parse::<SupportedPythonVersion>()
        .map_err(|_| UvMetadataError::InvalidPythonVersion(version.clone()))?;

    Ok(RangedValue::new(version, ValueSource::UvMetadata))
}

/// The uv metadata used to discover the workspace and check imports against its dependencies.
///
/// See uv's [schema documentation] and [serialization types] for the upstream format.
///
/// [schema documentation]: https://docs.astral.sh/uv/reference/internals/metadata/#schema
/// [serialization types]: https://github.com/astral-sh/uv/blob/main/crates/uv-resolver/src/lock/export/metadata.rs
#[derive(Deserialize)]
struct WorkspaceMetadata {
    workspace_root: PathBuf,
    #[serde(default)]
    members: Box<[WorkspaceMember]>,
    environment: Option<WorkspaceEnvironment>,
    schema: Schema,
    workspace: Option<NodeReference>,
    #[serde(default)]
    resolution: BTreeMap<CompactString, ResolutionNode>,
    #[serde(default)]
    module_owners: BTreeMap<CompactString, Box<[ModuleOwner]>>,
}

#[derive(Deserialize)]
struct WorkspaceEnvironment {
    root: PathBuf,
    python: WorkspacePython,
}

#[derive(Deserialize)]
struct WorkspacePython {
    version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct Schema {
    version: SchemaVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
#[serde(rename_all = "snake_case")]
enum SchemaVersion {
    Preview,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct ModuleOwner {
    package_id: CompactString,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct ResolutionNode {
    kind: NodeKind,
    name: Option<CompactString>,
    source: Option<Source>,
    // uv always emits this field, even for leaves. Missing edges are incomplete metadata, not
    // evidence that a project has no direct dependencies.
    dependencies: Box<[NodeReference]>,
    #[serde(default)]
    optional_dependencies: Box<[NodeReference]>,
    #[serde(default)]
    dependency_groups: Box<[NodeReference]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
#[serde(rename_all = "snake_case")]
enum NodeKind {
    Package,
    Extra(CompactString),
    Group(CompactString),
    Workspace,
    Script,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct Source {
    editable: Option<SystemPathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct NodeReference {
    id: CompactString,
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use ruff_db::system::{SystemPath, TestSystem};
    use serde_json::json;

    use super::{UvMetadata, UvMetadataError};

    #[test]
    fn rejects_invalid_metadata() {
        let system = TestSystem::default();

        assert_matches!(
            UvMetadata::from_metadata(b"{", &system),
            Err(UvMetadataError::InvalidMetadata(_))
        );
    }

    #[test]
    fn environment_can_be_omitted() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .write_file_all("/app/pyproject.toml", "[tool.uv.workspace]")?;
        let metadata = br#"{
            "schema": {"version": "preview"},
            "workspace_root": "/app"
        }"#;

        let workspace = UvMetadata::from_metadata(metadata, &system)?;

        assert!(workspace.environment().is_none());
        assert!(workspace.python_version().is_none());
        assert!(workspace.members().is_empty());
        assert!(workspace.dependency_metadata().is_err());

        Ok(())
    }

    #[test]
    fn uses_environment_python_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.memory_file_system().write_files_all([
            ("/app/pyproject.toml", "[tool.uv.workspace]"),
            ("/env/marker", ""),
        ])?;
        let metadata = br#"{
            "schema": {"version": "preview"},
            "workspace_root": "/app",
            "environment": {
                "root": "/env",
                "python": { "version": "3.13.5" }
            }
        }"#;

        let workspace = UvMetadata::from_metadata(metadata, &system)?;

        assert_eq!(workspace.environment(), Some(SystemPath::new("/env")));
        assert_eq!(
            workspace.python_version().map(ToString::to_string),
            Some("3.13".to_string())
        );

        Ok(())
    }

    #[test]
    fn rejects_unsupported_environment_python_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.memory_file_system().write_files_all([
            ("/app/pyproject.toml", "[tool.uv.workspace]"),
            ("/env/marker", ""),
        ])?;
        let metadata = br#"{
            "schema": {"version": "preview"},
            "workspace_root": "/app",
            "environment": {
                "root": "/env",
                "python": { "version": "3.16.0" }
            }
        }"#;

        assert_matches!(
            UvMetadata::from_metadata(metadata, &system),
            Err(UvMetadataError::InvalidPythonVersion(_))
        );

        Ok(())
    }

    #[test]
    fn rejects_incompatible_dependency_metadata() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.memory_file_system().write_files_all([
            ("/app/pyproject.toml", "[tool.uv.workspace]"),
            ("/env/marker", ""),
        ])?;
        for (schema, resolution) in [
            ("future-version", json!({})),
            ("preview", json!(["a different format"])),
        ] {
            let metadata = json!({
                "workspace_root": "/app",
                "environment": {
                    "root": "/env",
                    "python": { "version": "3.13.5" }
                },
                "schema": { "version": schema },
                "resolution": resolution
            });

            let metadata = serde_json::to_string_pretty(&metadata)?;
            let error = match UvMetadata::from_metadata(metadata.as_bytes(), &system) {
                Err(UvMetadataError::InvalidMetadata(error)) => error,
                result => anyhow::bail!("expected invalid metadata, got {result:?}"),
            };
            assert!(
                error.line() > 0 && error.line() < metadata.lines().count(),
                "expected the error to point to its field, not the end of the response: {error}"
            );
        }

        Ok(())
    }
}
