use std::path::PathBuf;
use std::process::Output;

use pep440_rs::Version;
use ruff_db::system::{Command, CommandExecutor, System, SystemPath, SystemPathBuf, WhichError};
use ruff_ranged_value::{RangedValue, ValueSource};
use serde::Deserialize;
use thiserror::Error;
use ty_static::EnvVars;

use super::python_version::SupportedPythonVersion;

pub(crate) use runner::{ScriptEnvironmentCacheKey, UvExecutor};
pub use runner::{ScriptSyncResult, ScriptSyncTask, UvSyncService};

mod runner;

#[derive(Clone)]
pub(super) struct Uv {
    executable: String,
}

impl Uv {
    pub(super) fn new(system: &dyn System) -> Result<Self, WhichError> {
        let executable = match system.env_var(EnvVars::UV) {
            Ok(executable) => executable,
            Err(_) => system.which("uv")?.into_string(),
        };

        Ok(Self { executable })
    }

    /// Executes `uv workspace metadata` and parses and validates its output.
    pub(super) fn metadata(
        &self,
        system: &dyn System,
        target: MetadataTarget<'_>,
    ) -> Result<UvMetadata, UvMetadataError> {
        let output = system
            .command_executor()
            .ok_or_else(unsupported_command_execution)
            .and_then(|executor| self.execute(executor, target));
        Self::parse_metadata_output(system, output)
    }

    /// Executes `uv workspace metadata` without interpreting its output.
    ///
    /// This operation only requires a detached command executor, so it can run on a background
    /// worker.
    #[tracing::instrument(name = "Uv::execute", level = "debug", skip(self, executor))]
    pub(super) fn execute(
        &self,
        executor: &dyn CommandExecutor,
        target: MetadataTarget<'_>,
    ) -> std::io::Result<Output> {
        let mut command = Command::new(self.executable.as_str());
        command.args(["workspace", "metadata"]);

        match target {
            MetadataTarget::Workspace(path) => {
                // `uv check` has already selected and synchronized the environment. Keep this
                // query read-only so package selection and `--isolated` aren't overwritten.
                command.args(["--frozen", "--active"]).current_dir(path);
            }
            MetadataTarget::Script { path, python } => {
                command.args(["--sync", "--script", path.as_str()]);
                if let Some(python) = python {
                    command.args(["--python", python.as_str()]);
                }
                if let Some(parent) = path.parent() {
                    command.current_dir(parent);
                }
            }
        }

        tracing::debug!(
            "Running `{} {}`",
            command.get_executable(),
            command.get_args().join(" ")
        );

        let start = ruff_db::Instant::now();
        let output = executor.execute(command);

        tracing::debug!(
            "uv metadata completed in {:.3}s",
            start.elapsed().as_secs_f64()
        );

        output
    }

    /// Parses and validates the output returned by [`Self::execute`].
    pub(super) fn parse_metadata_output(
        system: &dyn System,
        output: std::io::Result<Output>,
    ) -> Result<UvMetadata, UvMetadataError> {
        let output = output.map_err(UvMetadataError::Invocation)?;

        if !output.status.success() {
            return Err(UvMetadataError::CommandFailed {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        UvMetadata::from_metadata(&output.stdout, system)
    }
}

pub(super) fn uv_executable_error(error: WhichError) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("failed to resolve uv executable: {error}"),
    )
}

pub(super) fn unsupported_command_execution() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "running commands is not supported by this system",
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) enum MetadataTarget<'path> {
    Workspace(&'path SystemPath),
    Script {
        path: &'path SystemPath,
        python: Option<&'path SystemPath>,
    },
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
    use ty_static::EnvVars;

    use super::{Uv, UvMetadata, UvMetadataError};

    #[test]
    fn explicit_uv_override_skips_path_lookup() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.set_env_var(EnvVars::UV, "custom-uv");

        let uv = Uv::new(&system)?;

        assert_eq!(uv.executable, "custom-uv");

        Ok(())
    }

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
