//! Runs uv commands and manages background script synchronization.

use std::process::Output;

use ruff_db::system::{Command, CommandExecutor, System, SystemPath, WhichError};
use ty_combine::Combine;
use ty_static::EnvVars;

pub(crate) use metadata::{UvMetadata, UvMetadataError};
pub(crate) use sync::{ScriptSyncRequest, ScriptSyncResult, ScriptSyncTask, UvSyncService};

mod metadata;
mod sync;

/// Controls which uv integrations ty uses.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    get_size2::GetSize,
    serde::Deserialize,
    serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum UseUv {
    /// Disable all uv integration.
    #[default]
    Off,

    /// Use uv to create environments for standalone scripts.
    ///
    /// This does not use uv for project discovery.
    Scripts,

    /// Use uv for project discovery and standalone script environments.
    On,
}

impl UseUv {
    /// Resolves the mode configured by the `TY_UV` environment variable.
    pub fn from_system(system: &dyn System) -> Self {
        match system.env_var(EnvVars::TY_UV).as_deref() {
            Ok("1" | "true") => Self::On,
            Ok("scripts") => Self::Scripts,
            _ => Self::Off,
        }
    }

    pub(super) const fn workspace_discovery_enabled(self) -> bool {
        matches!(self, Self::On)
    }

    pub(super) const fn script_environments_enabled(self) -> bool {
        matches!(self, Self::Scripts | Self::On)
    }
}

impl Combine for UseUv {
    fn combine_with(&mut self, other: Self) {
        *self = other;
    }
}

#[derive(Clone)]
pub(crate) struct Uv {
    executable: String,
}

impl Uv {
    pub(crate) fn new(system: &dyn System) -> Result<Self, WhichError> {
        let executable = match system.env_var(EnvVars::UV) {
            Ok(executable) => executable,
            Err(_) => system.which("uv")?.into_string(),
        };

        Ok(Self { executable })
    }

    /// Executes `uv workspace metadata` and parses and validates its output.
    pub(crate) fn metadata(
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
    pub(crate) fn execute(
        &self,
        executor: &dyn CommandExecutor,
        target: MetadataTarget<'_>,
    ) -> std::io::Result<Output> {
        let mut command = Command::new(self.executable.as_str());
        command.args(["workspace", "metadata", "--quiet"]);

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
    pub(crate) fn parse_metadata_output(
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

#[derive(Clone, Copy, Debug)]
pub(crate) enum MetadataTarget<'path> {
    Workspace(&'path SystemPath),
    Script {
        path: &'path SystemPath,
        python: Option<&'path SystemPath>,
    },
}

pub(crate) fn uv_executable_error(error: WhichError) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("failed to resolve uv executable: {error}"),
    )
}

fn unsupported_command_execution() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "running commands is not supported by this system",
    )
}

#[cfg(test)]
mod tests {
    use ruff_db::system::TestSystem;
    use ty_static::EnvVars;

    use super::{UseUv, Uv};

    #[test]
    fn use_uv_from_system() {
        let system = TestSystem::default();
        assert_eq!(UseUv::from_system(&system), UseUv::Off);

        system.set_env_var(EnvVars::TY_UV, "scripts");
        assert_eq!(UseUv::from_system(&system), UseUv::Scripts);

        system.set_env_var(EnvVars::TY_UV, "true");
        assert_eq!(UseUv::from_system(&system), UseUv::On);

        system.set_env_var(EnvVars::TY_UV, "off");
        assert_eq!(UseUv::from_system(&system), UseUv::Off);
    }

    #[test]
    fn explicit_uv_override_skips_path_lookup() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.set_env_var(EnvVars::UV, "custom-uv");

        let uv = Uv::new(&system)?;

        assert_eq!(uv.executable, "custom-uv");

        Ok(())
    }
}
