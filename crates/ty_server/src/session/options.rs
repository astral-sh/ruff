use lsp_types::Url;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ty_project::metadata::Options;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::value::{RangedValue, RelativePathBuf};

use crate::logging::LogLevel;

use super::settings::{GlobalSettings, WorkspaceSettings};

/// Static initialization options that are set once at server startup that never change.
///
/// These are equivalent to command-line arguments and configure fundamental server behavior. Any
/// changes to these options require a server restart to take effect.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub(crate) struct InitializationOptions {
    /// The log level for the language server.
    pub(crate) log_level: Option<LogLevel>,

    /// Path to the log file, defaults to stderr if not set.
    ///
    /// Tildes (`~`) and environment variables (e.g., `$HOME`) are expanded.
    pub(crate) log_file: Option<SystemPathBuf>,
}

impl InitializationOptions {
    /// Create the initialization options from the given JSON value that corresponds to the
    /// initialization options sent by the client.
    pub(crate) fn from_value(
        options: Option<Value>,
    ) -> (InitializationOptions, Option<serde_json::Error>) {
        let options =
            options.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default()));
        match serde_json::from_value(options) {
            Ok(options) => (options, None),
            Err(err) => (InitializationOptions::default(), Some(err)),
        }
    }
}

/// Options that configure the behavior of the language server.
///
/// This is the direct representation of the options that the client sends to the server when
/// asking for workspace configuration. These options are dynamic and can change during the runtime
/// of the server via the `workspace/didChangeConfiguration` notification.
///
/// Usually, these options are at per-workspace level, but these can also include options that
/// needs to be applied globally such as the diagnostic mode. It's included here because they can
/// be changed dynamically by the client.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientOptions {
    /// Diagnostic mode for the language server.
    ///
    /// This applies to all of the workspaces that are managed by the language server.
    diagnostic_mode: Option<DiagnosticMode>,

    /// Whether to disable language services like code completions, hover, etc.
    ///
    /// This is applied for individual workspaces.
    disable_language_services: Option<bool>,

    /// Information about the currently active Python environment in the VS Code Python extension.
    ///
    /// This is relevant only for VS Code and is populated by the ty VS Code extension.
    python_extension: Option<PythonExtension>,
}

impl ClientOptions {
    /// Returns the client settings that are relevant to the language server.
    pub(crate) fn into_settings(self) -> (GlobalSettings, WorkspaceSettings) {
        let overrides = self.python_extension.and_then(|extension| {
            let active_environment = extension.active_environment?;

            let mut overrides = ProjectOptionsOverrides::new(None, Options::default());

            overrides.fallback_python = if let Some(environment) = &active_environment.environment {
                environment.folder_uri.to_file_path().ok().and_then(|path| {
                    Some(RelativePathBuf::python_extension(
                        SystemPathBuf::from_path_buf(path).ok()?,
                    ))
                })
            } else {
                Some(RelativePathBuf::python_extension(
                    active_environment.executable.sys_prefix.clone(),
                ))
            };

            overrides.fallback_python_version =
                active_environment.version.as_ref().and_then(|version| {
                    Some(RangedValue::python_extension(PythonVersion::from((
                        u8::try_from(version.major).ok()?,
                        u8::try_from(version.minor).ok()?,
                    ))))
                });

            if let Some(python) = &overrides.fallback_python {
                tracing::debug!(
                    "Using the Python environment selected in the VS Code Python extension in case the configuration doesn't specify a Python environment: {python}",
                    python = python.path()
                );
            }

            if let Some(version) = &overrides.fallback_python_version {
                tracing::debug!(
                    "Using the Python version selected in the VS Code Python extension: {version} in case the configuration doesn't specify a Python version",
                );
            }

            Some(overrides)
        });

        (
            GlobalSettings {
                diagnostic_mode: self.diagnostic_mode.unwrap_or_default(),
            },
            WorkspaceSettings {
                disable_language_services: self.disable_language_services.unwrap_or_default(),
                overrides,
            },
        )
    }

    #[must_use]
    pub fn with_diagnostic_mode(mut self, diagnostic_mode: DiagnosticMode) -> Self {
        self.diagnostic_mode = Some(diagnostic_mode);
        self
    }
}

/// Diagnostic mode for the language server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticMode {
    /// Check only currently open files.
    #[default]
    OpenFilesOnly,
    /// Check all files in the workspace.
    Workspace,
}

impl DiagnosticMode {
    /// Returns `true` if the diagnostic mode is set to check all files in the workspace.
    pub(crate) const fn is_workspace(self) -> bool {
        matches!(self, DiagnosticMode::Workspace)
    }

    /// Returns `true` if the diagnostic mode is set to check only currently open files.
    pub(crate) const fn is_open_files_only(self) -> bool {
        matches!(self, DiagnosticMode::OpenFilesOnly)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PythonExtension {
    active_environment: Option<ActiveEnvironment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiveEnvironment {
    pub(crate) executable: PythonExecutable,
    pub(crate) environment: Option<PythonEnvironment>,
    pub(crate) version: Option<EnvironmentVersion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvironmentVersion {
    pub(crate) major: i64,
    pub(crate) minor: i64,
    #[allow(dead_code)]
    pub(crate) patch: i64,
    #[allow(dead_code)]
    pub(crate) sys_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PythonEnvironment {
    pub(crate) folder_uri: Url,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[allow(dead_code)]
    pub(crate) name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PythonExecutable {
    #[allow(dead_code)]
    pub(crate) uri: Url,
    pub(crate) sys_prefix: SystemPathBuf,
}
