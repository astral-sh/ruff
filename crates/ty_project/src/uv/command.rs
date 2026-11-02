//! Constructs and executes uv metadata commands.

use std::process::Output;

use pep440_rs::Version;
use ruff_db::system::{Command, CommandExecutor, System, SystemPath, WhichError};
use ty_static::EnvVars;

use super::{UvMetadata, UvMetadataError};

pub(super) const MINIMUM_UV_VERSION: [u64; 3] = [0, 12, 3];

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
        target: &MetadataTarget<'_>,
    ) -> Result<UvMetadata, UvMetadataError> {
        let output = system
            .command_executor()
            .ok_or_else(unsupported_command_execution)
            .map_err(UvMetadataError::Invocation)
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
        target: &MetadataTarget<'_>,
    ) -> Result<Output, UvMetadataError> {
        let mut command = Command::new(self.executable.as_str());
        command.args(["workspace", "metadata", "--quiet"]);

        let directory = match target {
            MetadataTarget::Workspace(path) => {
                // Use the environment selected by `uv check` without synchronizing it.
                // Let uv apply its configured lockfile policy.
                command.arg("--active");
                Some(*path)
            }
            MetadataTarget::Script { path, python } => {
                command.args(["--sync", "--script", path.as_str()]);
                if let Some(python) = python {
                    command.args(["--python", python.as_str()]);
                }
                path.parent()
            }
        };
        if let Some(directory) = directory {
            command.current_dir(directory);
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

        let output = output.map_err(UvMetadataError::Invocation)?;

        // Before uv 0.12.3, `--quiet` suppresses the metadata JSON even on success.
        if (!output.status.success() || output.stdout.is_empty())
            && let Some(version) = self.version(executor, directory)
            && version < Version::new(MINIMUM_UV_VERSION)
        {
            return Err(UvMetadataError::UnsupportedVersion {
                executable: self.executable.clone(),
                version,
            });
        }

        Ok(output)
    }

    fn version(
        &self,
        executor: &dyn CommandExecutor,
        directory: Option<&SystemPath>,
    ) -> Option<Version> {
        let mut command = Command::new(self.executable.as_str());
        command.arg("--version");
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        let output = executor.execute(command).ok()?;
        if !output.status.success() {
            return None;
        }

        std::str::from_utf8(&output.stdout)
            .ok()?
            .strip_prefix("uv ")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }

    /// Parses and validates the output returned by [`Self::execute`].
    pub(crate) fn parse_metadata_output(
        system: &dyn System,
        output: Result<Output, UvMetadataError>,
    ) -> Result<UvMetadata, UvMetadataError> {
        let output = output?;

        if !output.status.success() {
            return Err(UvMetadataError::CommandFailed {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        UvMetadata::from_metadata(&output.stdout, system)
    }
}

/// The workspace or standalone script for which to request uv metadata.
#[derive(Debug)]
pub(crate) enum MetadataTarget<'path> {
    /// The directory from which uv discovers the workspace, not necessarily the workspace root.
    Workspace(&'path SystemPath),
    /// A standalone Python script.
    Script {
        /// The script file passed to `--script`.
        path: &'path SystemPath,
        /// The optional `--python` argument.
        python: Option<&'path SystemPath>,
    },
}

pub(crate) fn uv_executable_error(error: WhichError) -> std::io::Error {
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

#[cfg(test)]
mod tests {
    use ruff_db::system::TestSystem;
    use ty_static::EnvVars;

    use super::Uv;

    #[test]
    fn explicit_uv_override_skips_path_lookup() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.set_env_var(EnvVars::UV, "custom-uv");

        let uv = Uv::new(&system)?;

        assert_eq!(uv.executable, "custom-uv");

        Ok(())
    }
}
