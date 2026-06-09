use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use ruff_db::system::{SystemPath, SystemPathBuf};
use serde::Deserialize;
use ty_static::EnvVars;

pub(crate) struct UvWorkspace {
    pub(crate) root: SystemPathBuf,
    pub(crate) member: Option<SystemPathBuf>,
}

pub(crate) enum WorkspaceMetadataSource {
    Command,
    Stdin,
}

pub(crate) fn discover_workspace(
    cwd: &SystemPath,
    metadata_source: WorkspaceMetadataSource,
) -> Option<UvWorkspace> {
    let metadata = match metadata_source {
        WorkspaceMetadataSource::Command => {
            if !matches!(std::env::var(EnvVars::TY_UV).as_deref(), Ok("1" | "true")) {
                return None;
            }

            let uv = std::env::var_os(EnvVars::UV).unwrap_or_else(|| "uv".into());

            let output = match Command::new(uv)
                .arg("workspace")
                .arg("metadata")
                .current_dir(cwd.as_std_path())
                .output()
            {
                Ok(output) => output,
                Err(error) => {
                    tracing::debug!("Failed to invoke `uv workspace metadata`: {error}");
                    return None;
                }
            };

            if !output.status.success() {
                tracing::debug!(
                    "`uv workspace metadata` failed with status {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
                return None;
            }

            output.stdout
        }
        WorkspaceMetadataSource::Stdin => {
            let mut metadata = Vec::new();
            if let Err(error) = std::io::stdin().read_to_end(&mut metadata) {
                tracing::debug!(
                    "Failed to read `uv workspace metadata` output from stdin: {error}"
                );
                return None;
            }
            metadata
        }
    };

    parse_workspace_metadata(cwd, &metadata)
}

fn parse_workspace_metadata(cwd: &SystemPath, metadata: &[u8]) -> Option<UvWorkspace> {
    let metadata = match serde_json::from_slice::<WorkspaceMetadata>(metadata) {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::debug!("Failed to parse `uv workspace metadata` output: {error}");
            return None;
        }
    };

    if metadata.schema.version != "preview" {
        tracing::debug!(
            "Ignoring unsupported `uv workspace metadata` schema version `{}`",
            metadata.schema.version
        );
        return None;
    }

    let root = match SystemPathBuf::from_path_buf(metadata.workspace_root) {
        Ok(root) => root,
        Err(root) => {
            tracing::debug!(
                "Ignoring non-Unicode workspace root returned by `uv workspace metadata`: `{}`",
                root.display()
            );
            return None;
        }
    };

    if !root.as_std_path().is_dir() {
        tracing::debug!(
            "Ignoring missing workspace root returned by `uv workspace metadata`: `{root}`"
        );
        return None;
    }

    let member = metadata
        .members
        .into_iter()
        .map(|member| member.path)
        .filter(|member| cwd.as_std_path().starts_with(member))
        .max_by_key(|member| member.components().count());
    let member = match member {
        Some(member) => {
            let member = match SystemPathBuf::from_path_buf(member) {
                Ok(member) => member,
                Err(member) => {
                    tracing::debug!(
                        "Ignoring non-Unicode workspace member returned by `uv workspace metadata`: `{}`",
                        member.display()
                    );
                    return None;
                }
            };

            if !member.as_std_path().is_dir() {
                tracing::debug!(
                    "Ignoring missing workspace member returned by `uv workspace metadata`: `{member}`"
                );
                return None;
            }

            Some(member)
        }
        None => None,
    };

    Some(UvWorkspace { root, member })
}

#[derive(Deserialize)]
struct WorkspaceMetadata {
    schema: WorkspaceMetadataSchema,
    workspace_root: PathBuf,
    members: Vec<WorkspaceMember>,
}

#[derive(Deserialize)]
struct WorkspaceMetadataSchema {
    version: String,
}

#[derive(Deserialize)]
struct WorkspaceMember {
    path: PathBuf,
}
