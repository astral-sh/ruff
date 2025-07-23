use lsp_types::Url;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use ty_project::CheckMode;
use ty_project::metadata::Options;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::value::{RangedValue, RelativePathBuf};

use crate::logging::LogLevel;
use crate::session::ClientSettings;

pub(crate) type WorkspaceOptionsMap = FxHashMap<Url, ClientOptions>;

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalOptions {
    #[serde(flatten)]
    client: ClientOptions,

    // These settings are only needed for tracing, and are only read from the global configuration.
    // These will not be in the resolved settings.
    #[serde(flatten)]
    pub(crate) tracing: TracingOptions,
}

impl GlobalOptions {
    pub(crate) fn into_settings(self) -> ClientSettings {
        self.client.into_settings()
    }

    pub(crate) fn diagnostic_mode(&self) -> DiagnosticMode {
        self.client.diagnostic_mode.unwrap_or_default()
    }
}

/// This is a direct representation of the workspace settings schema, which inherits the schema of
/// [`ClientOptions`] and adds extra fields to describe the workspace it applies to.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct WorkspaceOptions {
    #[serde(flatten)]
    options: ClientOptions,
    workspace: Url,
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Clone, Debug, Deserialize, Default)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientOptions {
    /// Settings under the `python.*` namespace in VS Code that are useful for the ty language
    /// server.
    python: Option<Python>,
    /// Diagnostic mode for the language server.
    diagnostic_mode: Option<DiagnosticMode>,

    python_extension: Option<PythonExtension>,
}

/// Diagnostic mode for the language server.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) enum DiagnosticMode {
    /// Check only currently open files.
    #[default]
    OpenFilesOnly,
    /// Check all files in the workspace.
    Workspace,
}

impl DiagnosticMode {
    pub(crate) fn is_workspace(self) -> bool {
        matches!(self, DiagnosticMode::Workspace)
    }

    pub(crate) fn into_check_mode(self) -> CheckMode {
        match self {
            DiagnosticMode::OpenFilesOnly => CheckMode::OpenFiles,
            DiagnosticMode::Workspace => CheckMode::AllFiles,
        }
    }
}

impl ClientOptions {
    /// Returns the client settings that are relevant to the language server.
    pub(crate) fn into_settings(self) -> ClientSettings {
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

        ClientSettings {
            disable_language_services: self
                .python
                .and_then(|python| python.ty)
                .and_then(|ty| ty.disable_language_services)
                .unwrap_or_default(),
            diagnostic_mode: self.diagnostic_mode.unwrap_or_default(),
            overrides,
        }
    }
}

// TODO(dhruvmanila): We need to mirror the "python.*" namespace on the server side but ideally it
// would be useful to instead use `workspace/configuration` instead. This would be then used to get
// all settings and not just the ones in "python.*".

#[derive(Clone, Debug, Deserialize, Default)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Python {
    ty: Option<Ty>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct PythonExtension {
    active_environment: Option<ActiveEnvironment>,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiveEnvironment {
    pub(crate) executable: PythonExecutable,
    pub(crate) environment: Option<PythonEnvironment>,
    pub(crate) version: Option<EnvironmentVersion>,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvironmentVersion {
    pub(crate) major: i64,
    pub(crate) minor: i64,
    #[allow(dead_code)]
    pub(crate) patch: i64,
    #[allow(dead_code)]
    pub(crate) sys_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PythonEnvironment {
    pub(crate) folder_uri: Url,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[allow(dead_code)]
    pub(crate) name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PythonExecutable {
    #[allow(dead_code)]
    pub(crate) uri: Url,
    pub(crate) sys_prefix: SystemPathBuf,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[cfg_attr(test, derive(serde::Serialize, PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct Ty {
    disable_language_services: Option<bool>,
}

/// This is a direct representation of the settings schema sent by the client.
/// Settings needed to initialize tracing. These will only be read from the global configuration.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TracingOptions {
    pub(crate) log_level: Option<LogLevel>,

    /// Path to the log file - tildes and environment variables are supported.
    pub(crate) log_file: Option<SystemPathBuf>,
}

/// This is the exact schema for initialization options sent in by the client during
/// initialization.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(untagged)]
enum InitializationOptions {
    #[serde(rename_all = "camelCase")]
    HasWorkspaces {
        #[serde(rename = "globalSettings")]
        global: GlobalOptions,
        #[serde(rename = "settings")]
        workspace: Vec<WorkspaceOptions>,
    },
    GlobalOnly {
        #[serde(default)]
        settings: GlobalOptions,
    },
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly {
            settings: GlobalOptions::default(),
        }
    }
}

/// Built from the initialization options provided by the client.
#[derive(Debug)]
pub(crate) struct AllOptions {
    pub(crate) global: GlobalOptions,
    /// If this is `None`, the client only passed in global settings.
    pub(crate) workspace: Option<WorkspaceOptionsMap>,
}

impl AllOptions {
    /// Initializes the controller from the serialized initialization options. This fails if
    /// `options` are not valid initialization options.
    pub(crate) fn from_value(options: serde_json::Value) -> Self {
        Self::from_init_options(
            serde_json::from_value(options)
                .map_err(|err| {
                    tracing::error!("Failed to deserialize initialization options: {err}. Falling back to default client settings...");
                })
                .unwrap_or_default(),
        )
    }

    fn from_init_options(options: InitializationOptions) -> Self {
        let (global_options, workspace_options) = match options {
            InitializationOptions::GlobalOnly { settings: options } => (options, None),
            InitializationOptions::HasWorkspaces {
                global: global_options,
                workspace: workspace_options,
            } => (global_options, Some(workspace_options)),
        };

        Self {
            global: global_options,
            workspace: workspace_options.map(|workspace_options| {
                workspace_options
                    .into_iter()
                    .map(|workspace_options| {
                        (workspace_options.workspace, workspace_options.options)
                    })
                    .collect()
            }),
        }
    }
}
