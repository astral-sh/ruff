//! Constructs and executes uv metadata commands.

use std::process::Output;

use pep440_rs::Version;
use ruff_db::system::{Command, CommandExecutor, System, SystemPath, WhichError};
use ty_static::EnvVars;

use super::{UvMetadata, UvMetadataError};

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

    /// Executes `uv workspace metadata`, checking for an unsupported uv version on failure.
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

        match target {
            MetadataTarget::Workspace(path) => {
                // Use the environment selected by `uv check` without synchronizing it.
                // Let uv apply its configured lockfile policy.
                command.arg("--active").current_dir(path);
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

        let output = output.map_err(UvMetadataError::Invocation)?;

        // Before uv 0.12.3, `--quiet` suppresses the metadata JSON even on success.
        // Only probe the version when metadata is unavailable, so successful calls stay cheap.
        if (!output.status.success() || output.stdout.is_empty())
            && let Some(version) = self.version(executor, target)
        {
            let minimum_version = Version::new([0, 12, 3]);
            if version < minimum_version {
                return Err(UvMetadataError::UnsupportedVersion {
                    executable: self.executable.clone(),
                    version,
                    minimum_version,
                });
            }
        }

        Ok(output)
    }

    fn version(
        &self,
        executor: &dyn CommandExecutor,
        target: &MetadataTarget<'_>,
    ) -> Option<Version> {
        let mut command = Command::new(self.executable.as_str());
        command.arg("--version");
        let directory = match target {
            MetadataTarget::Workspace(path) => Some(*path),
            MetadataTarget::Script { path, .. } => path.parent(),
        };
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
    use std::collections::VecDeque;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};
    use std::sync::Arc;

    use parking_lot::Mutex;
    use ruff_db::system::{Command, CommandExecutor, SystemPath, TestSystem};
    use ty_static::EnvVars;

    use super::{MetadataTarget, Uv, UvMetadataError};

    #[derive(Clone, Default)]
    struct MockExecutor {
        outputs: Arc<Mutex<VecDeque<std::io::Result<Output>>>>,
        commands: Arc<Mutex<Vec<Command>>>,
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, command: Command) -> std::io::Result<Output> {
            self.commands.lock().push(command);
            self.outputs
                .lock()
                .pop_front()
                .unwrap_or_else(|| Err(std::io::Error::other("unexpected command")))
        }

        fn dyn_clone(&self) -> Box<dyn CommandExecutor> {
            Box::new(self.clone())
        }
    }

    fn output(success: bool, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw((!success).into()),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn explicit_uv_override_skips_path_lookup() -> anyhow::Result<()> {
        let system = TestSystem::default();
        system.set_env_var(EnvVars::UV, "custom-uv");

        let uv = Uv::new(&system)?;

        assert_eq!(uv.executable, "custom-uv");

        Ok(())
    }

    #[test]
    fn successful_metadata_does_not_probe_version() -> anyhow::Result<()> {
        let system = TestSystem::default();
        let root = SystemPath::new(if cfg!(windows) { "C:/app" } else { "/app" });
        system.memory_file_system().create_directory_all(root)?;
        let metadata = serde_json::json!({
            "schema": {"version": "preview"},
            "workspace_root": root.as_str(),
        });
        let executor = MockExecutor::default();
        executor
            .outputs
            .lock()
            .push_back(Ok(output(true, &metadata.to_string(), "")));
        let uv = Uv {
            executable: "custom-uv".into(),
        };

        let result = uv.execute(&executor, &MetadataTarget::Workspace(root));
        assert_eq!(
            Uv::parse_metadata_output(&system, result)?.workspace_root(),
            root
        );
        assert_eq!(executor.commands.lock().len(), 1);

        Ok(())
    }

    #[test]
    fn old_uv_reports_upgrade() {
        // Old uv can either reject the metadata command or succeed with no JSON due to --quiet.
        for metadata in [
            output(false, "", "unrecognized subcommand"),
            output(true, "", ""),
        ] {
            let executor = MockExecutor::default();
            executor.outputs.lock().extend([
                Ok(metadata),
                Ok(output(true, "uv 0.12.2 (abcdef 2026-08-05)\n", "")),
            ]);
            let uv = Uv {
                executable: "custom-uv".into(),
            };
            let result = uv.execute(
                &executor,
                &MetadataTarget::Script {
                    path: SystemPath::new("/app/script.py"),
                    python: None,
                },
            );

            let error = result.unwrap_err();
            assert_eq!(
                error.to_string(),
                "uv 0.12.2 is too old; upgrade `custom-uv` to uv 0.12.3 or newer"
            );
            let commands = executor.commands.lock();
            assert_eq!(commands.len(), 2);
            assert!(
                commands
                    .iter()
                    .all(|command| command.get_executable() == "custom-uv")
            );
            assert_eq!(commands[1].get_args(), ["--version"]);
            assert_eq!(commands[1].get_current_dir(), commands[0].get_current_dir());
        }
    }

    #[test]
    fn preserves_errors_without_an_unsupported_version() {
        for version in [
            Ok(output(true, "uv 0.12.3\n", "")),
            Ok(output(true, "uv 0.12.19 (abcdef 2026-09-24)\n", "")),
            Ok(output(true, "uv 1.0.0\n", "")),
            Ok(output(true, "unknown version\n", "")),
            Ok(output(false, "uv 0.12.2\n", "version failed")),
            Err(std::io::Error::other("version unavailable")),
        ] {
            let executor = MockExecutor::default();
            let metadata = output(false, "", "dependency could not be resolved");
            let status = metadata.status;
            executor.outputs.lock().extend([Ok(metadata), version]);
            let uv = Uv {
                executable: "custom-uv".into(),
            };
            let result = uv.execute(
                &executor,
                &MetadataTarget::Workspace(SystemPath::new("/app")),
            );
            let error = Uv::parse_metadata_output(&TestSystem::default(), result).unwrap_err();
            assert!(
                matches!(error, UvMetadataError::CommandFailed { status: actual, stderr }
                if actual == status && stderr == "dependency could not be resolved")
            );
        }
    }

    #[test]
    fn preserves_invalid_metadata_from_supported_uv() {
        for stdout in ["", "invalid JSON"] {
            let executor = MockExecutor::default();
            executor.outputs.lock().extend([
                Ok(output(true, stdout, "")),
                Ok(output(true, "uv 0.12.3\n", "")),
            ]);
            let uv = Uv {
                executable: "custom-uv".into(),
            };
            let result = uv.execute(
                &executor,
                &MetadataTarget::Workspace(SystemPath::new("/app")),
            );
            assert!(matches!(
                Uv::parse_metadata_output(&TestSystem::default(), result),
                Err(UvMetadataError::InvalidMetadata(_))
            ));
            assert_eq!(
                executor.commands.lock().len(),
                if stdout.is_empty() { 2 } else { 1 }
            );
        }
    }
}
