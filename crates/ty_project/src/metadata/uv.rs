use std::path::PathBuf;

use pep440_rs::Version;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_ranged_value::{RangedValue, ValueSource};
use serde::Deserialize;
use thiserror::Error;
use ty_static::EnvVars;

use super::python_version::SupportedPythonVersion;

pub(super) struct Uv<'system> {
    system: &'system dyn System,
}

impl<'system> Uv<'system> {
    pub(super) fn new(system: &'system dyn System) -> Self {
        Self { system }
    }

    pub(super) fn metadata(
        &self,
        target: MetadataTarget<'_>,
    ) -> Result<UvMetadata, UvMetadataError> {
        match target {
            MetadataTarget::Workspace(path) => {
                // `uv check` has already selected and synchronized the environment. Keep this
                // query read-only so package selection and `--isolated` aren't overwritten.
                self.run_metadata(path, &["workspace", "metadata", "--frozen", "--active"])
            }
            MetadataTarget::Script(path) => {
                let directory = path
                    .parent()
                    .unwrap_or_else(|| self.system.current_directory());
                self.run_metadata(
                    directory,
                    &["workspace", "metadata", "--sync", "--script", path.as_str()],
                )
            }
        }
    }

    #[tracing::instrument(name = "Uv::run_metadata", level = "debug", skip(self))]
    fn run_metadata(
        &self,
        directory: &SystemPath,
        arguments: &[&str],
    ) -> Result<UvMetadata, UvMetadataError> {
        let uv = self
            .system
            .env_var(EnvVars::UV)
            .unwrap_or_else(|_| "uv".to_string());

        tracing::debug!("Running `{uv} {}`", arguments.join(" "));

        let start = ruff_db::Instant::now();
        let output = self.system.run_command(&uv, arguments, directory);

        tracing::debug!(
            "`{uv} {}` completed in {:.3}s",
            arguments.join(" "),
            start.elapsed().as_secs_f64()
        );

        let output = output.map_err(UvMetadataError::Invocation)?;

        if !output.status.success() {
            return Err(UvMetadataError::CommandFailed {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        UvMetadata::from_metadata(&output.stdout, self.system)
    }
}

#[derive(Clone, Copy)]
pub(super) enum MetadataTarget<'path> {
    Workspace(&'path SystemPath),
    Script(&'path SystemPath),
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct UvMetadata {
    workspace_root: SystemPathBuf,
    environment: Option<SystemPathBuf>,
    python_version: Option<RangedValue<SupportedPythonVersion>>,
}

impl UvMetadata {
    pub(super) fn workspace_root(&self) -> &SystemPath {
        &self.workspace_root
    }

    pub(super) fn environment(&self) -> Option<&SystemPath> {
        self.environment.as_deref()
    }

    pub(super) fn python_version(&self) -> Option<&RangedValue<SupportedPythonVersion>> {
        self.python_version.as_ref()
    }

    pub(super) fn from_metadata(
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
            environment,
            python_version,
        })
    }
}

#[derive(Debug, Error)]
pub(super) enum UvMetadataError {
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

#[derive(Deserialize)]
struct WorkspaceMetadata {
    workspace_root: PathBuf,
    environment: Option<WorkspaceEnvironment>,
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

#[cfg(test)]
mod tests {
    use ruff_db::system::{SystemPath, TestSystem};

    use super::{UvMetadata, UvMetadataError};

    #[test]
    fn rejects_invalid_metadata() {
        let system = TestSystem::default();

        assert!(matches!(
            UvMetadata::from_metadata(b"{", &system),
            Err(UvMetadataError::InvalidMetadata(_))
        ));
    }

    #[test]
    fn environment_can_be_omitted() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system
            .memory_file_system()
            .write_file_all("/app/pyproject.toml", "[tool.uv.workspace]")?;
        let metadata = br#"{
            "workspace_root": "/app"
        }"#;

        let workspace = UvMetadata::from_metadata(metadata, &system)?;

        assert!(workspace.environment().is_none());
        assert!(workspace.python_version().is_none());

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
            "workspace_root": "/app",
            "environment": {
                "root": "/env",
                "python": { "version": "3.16.0" }
            }
        }"#;

        assert!(matches!(
            UvMetadata::from_metadata(metadata, &system),
            Err(UvMetadataError::InvalidPythonVersion(_))
        ));

        Ok(())
    }
}
