use std::path::PathBuf;
use std::process::Command;

use ruff_db::system::{SystemPath, SystemPathBuf};
use serde::Deserialize;
use ty_static::EnvVars;

pub(crate) fn discover_workspace_root(cwd: &SystemPath) -> Option<SystemPathBuf> {
    let uv = std::env::var_os(EnvVars::UV)?;

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

    let metadata = match serde_json::from_slice::<WorkspaceMetadata>(&output.stdout) {
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

    Some(root)
}

#[derive(Deserialize)]
struct WorkspaceMetadata {
    schema: WorkspaceMetadataSchema,
    workspace_root: PathBuf,
}

#[derive(Deserialize)]
struct WorkspaceMetadataSchema {
    version: String,
}
