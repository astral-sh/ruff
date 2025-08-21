use std::collections::HashMap;

use lsp_types::Url;
use ruff_db::system::SystemPathBuf;
use ruff_macros::Combine;
use ruff_python_ast::PythonVersion;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ty_combine::Combine;
use ty_ide::InlayHintSettings;
use ty_project::metadata::Options as TyOptions;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::value::{RangedValue, RelativePathBuf};

use crate::logging::LogLevel;

use super::settings::{ExperimentalSettings, GlobalSettings, WorkspaceSettings};

/// Initialization options that are set once at server startup that never change.
///
/// There are two sets of options that are defined here:
/// 1. Options that are static, set once and are required at server startup. Any changes to these
///    options require a server restart to take effect.
/// 2. Options that are dynamic and can change during the runtime of the server, such as the
///    diagnostic mode.
///
/// The dynamic options are also accepted during the initialization phase, so that we can support
/// clients that do not support the `workspace/didChangeConfiguration` notification.
///
/// Note that this structure has a limitation in that there's no way to specify different options
/// for different workspaces in the initialization options which means that the server will not
/// support multiple workspaces for clients that do not implement the `workspace/configuration`
/// endpoint. Most editors support this endpoint, so this is not a significant limitation.
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InitializationOptions {
    /// The log level for the language server.
    pub(crate) log_level: Option<LogLevel>,

    /// Path to the log file, defaults to stderr if not set.
    ///
    /// Tildes (`~`) and environment variables (e.g., `$HOME`) are expanded.
    pub(crate) log_file: Option<SystemPathBuf>,

    /// The remaining options that are dynamic and can change during the runtime of the server.
    #[serde(flatten)]
    pub(crate) options: ClientOptions,
}

impl InitializationOptions {
    /// Create the initialization options from the given JSON value that corresponds to the
    /// initialization options sent by the client.
    ///
    /// It returns a tuple of the initialization options and an optional error if the JSON value
    /// could not be deserialized into the initialization options. In case of an error, the default
    /// initialization options are returned.
    pub(crate) fn from_value(
        options: Option<Value>,
    ) -> (InitializationOptions, Option<serde_json::Error>) {
        let Some(options) = options else {
            return (InitializationOptions::default(), None);
        };
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
/// The representation of the options is split into two parts:
/// 1. Global options contains options that are global to the language server. They are applied to
///    all workspaces managed by the language server.
/// 2. Workspace options contains options that are specific to a workspace. They are applied to the
///    workspace these options are associated with.
#[derive(Clone, Combine, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientOptions {
    #[serde(flatten)]
    pub(crate) global: GlobalOptions,

    #[serde(flatten)]
    pub(crate) workspace: WorkspaceOptions,

    /// Additional options that aren't valid as per the schema but we accept it to provide better
    /// error message to the user.
    #[serde(flatten)]
    pub(crate) unknown: HashMap<String, Value>,
}

impl ClientOptions {
    #[must_use]
    pub fn with_diagnostic_mode(mut self, diagnostic_mode: DiagnosticMode) -> Self {
        self.global.diagnostic_mode = Some(diagnostic_mode);
        self
    }

    #[must_use]
    pub fn with_disable_language_services(mut self, disable_language_services: bool) -> Self {
        self.workspace.disable_language_services = Some(disable_language_services);
        self
    }

    #[must_use]
    pub fn with_variable_types_inlay_hints(mut self, variable_types: bool) -> Self {
        self.workspace
            .inlay_hints
            .get_or_insert_default()
            .variable_types = Some(variable_types);
        self
    }

    #[must_use]
    pub fn with_experimental_rename(mut self, enabled: bool) -> Self {
        self.global.experimental.get_or_insert_default().rename = Some(enabled);
        self
    }

    #[must_use]
    pub fn with_unknown(mut self, unknown: HashMap<String, Value>) -> Self {
        self.unknown = unknown;
        self
    }
}

/// Options that are global to the language server.
///
/// These are the dynamic options that are applied to all workspaces managed by the language
/// server.
#[derive(Clone, Combine, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalOptions {
    /// Diagnostic mode for the language server.
    diagnostic_mode: Option<DiagnosticMode>,

    /// Experimental features that the server provides on an opt-in basis.
    pub(crate) experimental: Option<Experimental>,
}

impl GlobalOptions {
    pub(crate) fn into_settings(self) -> GlobalSettings {
        let experimental = self
            .experimental
            .map(|experimental| ExperimentalSettings {
                rename: experimental.rename.unwrap_or(true),
            })
            .unwrap_or_default();

        GlobalSettings {
            diagnostic_mode: self.diagnostic_mode.unwrap_or_default(),
            experimental,
        }
    }
}

/// Options that are specific to a workspace.
///
/// These are the dynamic options that are applied to a specific workspace.
#[derive(Clone, Combine, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceOptions {
    /// Whether to disable language services like code completions, hover, etc.
    disable_language_services: Option<bool>,

    /// Options to configure inlay hints.
    inlay_hints: Option<InlayHintOptions>,

    /// Information about the currently active Python environment in the VS Code Python extension.
    ///
    /// This is relevant only for VS Code and is populated by the ty VS Code extension.
    python_extension: Option<PythonExtension>,
}

impl WorkspaceOptions {
    pub(crate) fn into_settings(self) -> WorkspaceSettings {
        let overrides = self.python_extension.and_then(|extension| {
            let active_environment = extension.active_environment?;

            let mut overrides = ProjectOptionsOverrides::new(None, TyOptions::default());

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
                    "Using the Python environment selected in the VS Code Python extension \
                    in case the configuration doesn't specify a Python environment: {python}",
                    python = python.path()
                );
            }

            if let Some(version) = &overrides.fallback_python_version {
                tracing::debug!(
                    "Using the Python version selected in the VS Code Python extension: {version} \
                    in case the configuration doesn't specify a Python version",
                );
            }

            Some(overrides)
        });

        WorkspaceSettings {
            disable_language_services: self.disable_language_services.unwrap_or_default(),
            inlay_hints: self
                .inlay_hints
                .map(InlayHintOptions::into_settings)
                .unwrap_or_default(),
            overrides,
        }
    }
}

#[derive(Clone, Combine, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct InlayHintOptions {
    variable_types: Option<bool>,
    call_argument_names: Option<bool>,
}

impl InlayHintOptions {
    fn into_settings(self) -> InlayHintSettings {
        InlayHintSettings {
            variable_types: self.variable_types.unwrap_or(true),
            call_argument_names: self.call_argument_names.unwrap_or(true),
        }
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

impl Combine for DiagnosticMode {
    fn combine_with(&mut self, other: Self) {
        // Diagnostic mode is a global option but as it can be updated without a server restart,
        // it is part of the dynamic option set. But, there's no easy way to enforce the fact that
        // this option should not be set for individual workspaces. The ty VS Code extension
        // enforces this but we're not in control of other clients.
        //
        // So, this is a workaround to ensure that if the diagnostic mode is set to `workspace` in
        // either an initialization options or one of the workspace options, it is always set to
        // `workspace` in the global options.
        if other.is_workspace() {
            *self = DiagnosticMode::Workspace;
        }
    }
}

#[derive(Clone, Combine, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Experimental {
    /// Whether to enable the experimental symbol rename feature.
    pub(crate) rename: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PythonExtension {
    active_environment: Option<ActiveEnvironment>,
}

impl Combine for PythonExtension {
    fn combine_with(&mut self, _other: Self) {
        panic!(
            "`python_extension` is not expected to be combined with the initialization options as \
            it's only set by the ty VS Code extension in the `workspace/configuration` request."
        );
    }
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
