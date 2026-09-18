use std::collections::BTreeMap;
use std::path::PathBuf;

use char_str::CharStr;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use serde::Deserialize;
use thiserror::Error;

mod dependencies;
mod string_interner;

pub(crate) use dependencies::DependencyMetadataError;

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct UvMetadata {
    workspace_root: SystemPathBuf,
    members: Box<[WorkspaceMember]>,
    environment: Option<SystemPathBuf>,
    schema: Schema,
    workspace: Option<NodeReference>,
    script: Option<PathNodeReference>,
    resolution: BTreeMap<CharStr, ResolutionNode>,
    module_owners: BTreeMap<CharStr, Box<[ModuleOwner]>>,
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

    pub(crate) fn from_metadata(
        metadata: &[u8],
        system: &dyn System,
    ) -> Result<Self, UvMetadataError> {
        let metadata: WorkspaceMetadata = {
            let _interner = string_interner::InternerGuard::new();
            serde_json::from_slice(metadata).map_err(UvMetadataError::InvalidMetadata)?
        };

        let workspace_root = existing_directory(metadata.workspace_root, "workspace root", system)?;

        let environment = match metadata.environment {
            Some(environment) => Some(existing_directory(
                environment.root,
                "environment root",
                system,
            )?),
            None => None,
        };

        Ok(Self {
            workspace_root,
            members: metadata.members,
            environment,
            schema: metadata.schema,
            workspace: metadata.workspace,
            script: metadata.script,
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
    #[serde(deserialize_with = "string_interner::deserialize")]
    id: CharStr,
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
    script: Option<PathNodeReference>,
    #[serde(default, deserialize_with = "string_interner::deserialize_map")]
    resolution: BTreeMap<CharStr, ResolutionNode>,
    #[serde(default, deserialize_with = "string_interner::deserialize_map")]
    module_owners: BTreeMap<CharStr, Box<[ModuleOwner]>>,
}

#[derive(Deserialize)]
struct WorkspaceEnvironment {
    root: PathBuf,
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
struct PathNodeReference {
    path: SystemPathBuf,
    #[serde(deserialize_with = "string_interner::deserialize")]
    id: CharStr,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct ModuleOwner {
    #[serde(deserialize_with = "string_interner::deserialize")]
    package_id: CharStr,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, get_size2::GetSize)]
struct ResolutionNode {
    kind: NodeKind,
    #[serde(default, deserialize_with = "string_interner::deserialize_optional")]
    name: Option<CharStr>,
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
    Extra(#[serde(deserialize_with = "string_interner::deserialize")] CharStr),
    Group(#[serde(deserialize_with = "string_interner::deserialize")] CharStr),
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
    #[serde(deserialize_with = "string_interner::deserialize")]
    id: CharStr,
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use anyhow::Context;
    use char_str::CharStr;
    use ruff_db::system::TestSystem;
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
        assert!(workspace.members().is_empty());
        assert!(workspace.dependency_metadata().is_err());

        Ok(())
    }

    fn assert_shared(expected: &CharStr, actual: &CharStr) {
        assert_eq!(actual, expected);
        assert!(expected.is_heap_allocated());
        assert!(std::ptr::eq(actual.as_str(), expected.as_str()));
    }

    #[test]
    fn shares_dependency_strings() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = if cfg!(windows) { "C:/app" } else { "/app" };
        system.memory_file_system().create_directory_all(root)?;

        let member_id = "app";
        let dependency_id = "shareddistributionname==1.0.0 (registry:first)";
        let name = "shareddistributionname";
        let input = serde_json::to_vec(&json!({
            "schema": {"version": "preview"},
            "workspace_root": root,
            "members": [{"id": member_id, "name": "app", "path": root}],
            "resolution": {
                (member_id): {
                    "kind": "package", "name": "app",
                    "dependencies": [{"id": dependency_id}]
                },
                (dependency_id): {"kind": "package", "name": name, "dependencies": []}
            },
            "module_owners": {
                (name): [{"package_id": dependency_id}]
            }
        }))?;

        let metadata = UvMetadata::from_metadata(&input, &system)?;
        let (dependency_key, _) = metadata
            .resolution
            .get_key_value(dependency_id)
            .context("expected the direct dependency")?;
        let (module_name, owners) = metadata
            .module_owners
            .get_key_value(name)
            .context("expected module ownership")?;

        assert_shared(
            dependency_key,
            &metadata.resolution[member_id].dependencies[0].id,
        );
        assert_shared(dependency_key, &owners[0].package_id);

        let dependencies = metadata.dependency_metadata()?;
        assert_shared(
            dependency_key,
            dependencies.projects[0]
                .dependencies
                .get(dependency_id)
                .context("expected a direct dependency")?,
        );
        assert_shared(module_name, &dependencies.distributions[dependency_id].name);

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
