use lsp_types::Url;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ty_project::metadata::Options as TyOptions;
use ty_project::metadata::options::ProjectOptionsOverrides;
use ty_project::metadata::value::{RangedValue, RelativePathBuf};

use crate::logging::LogLevel;

use super::settings::{GlobalSettings, WorkspaceSettings};

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
#[serde(deny_unknown_fields)]
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
/// The representation of the options is split into two parts:
/// 1. [`GlobalOptions`] which contains options that are global to the language server. They are
///    applied to all workspaces managed by the language server.
/// 2. [`WorkspaceOptions`] which contains options that are specific to a workspace. They are
///    applied to the workspace these options are associated with.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientOptions {
    #[serde(flatten)]
    pub(crate) global: GlobalOptions,

    #[serde(flatten)]
    pub(crate) workspace: WorkspaceOptions,
}

impl ClientOptions {
    #[must_use]
    pub fn with_diagnostic_mode(mut self, diagnostic_mode: DiagnosticMode) -> Self {
        self.global.diagnostic_mode = Some(diagnostic_mode);
        self
    }
}

impl Combine for ClientOptions {
    fn combine_with(&mut self, other: Self) {
        self.global.combine_with(other.global);
        self.workspace.combine_with(other.workspace);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalOptions {
    /// Diagnostic mode for the language server.
    diagnostic_mode: Option<DiagnosticMode>,
}

impl GlobalOptions {
    pub(crate) fn into_settings(self) -> GlobalSettings {
        GlobalSettings {
            diagnostic_mode: self.diagnostic_mode.unwrap_or_default(),
        }
    }
}

impl Combine for GlobalOptions {
    fn combine_with(&mut self, other: Self) {
        self.diagnostic_mode.combine_with(other.diagnostic_mode);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceOptions {
    /// Whether to disable language services like code completions, hover, etc.
    disable_language_services: Option<bool>,

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
            overrides,
        }
    }
}

impl Combine for WorkspaceOptions {
    fn combine_with(&mut self, other: Self) {
        self.disable_language_services
            .combine_with(other.disable_language_services);
        self.python_extension.combine_with(other.python_extension);
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
        if other.is_workspace() {
            *self = DiagnosticMode::Workspace;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PythonExtension {
    active_environment: Option<ActiveEnvironment>,
}

impl Combine for PythonExtension {
    fn combine_with(&mut self, _other: Self) {
        unreachable!(
            "`python_extension` is not expected to be combined with the intialization options as \
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

pub(crate) trait Combine {
    #[must_use]
    fn combine(mut self, other: Self) -> Self
    where
        Self: Sized,
    {
        self.combine_with(other);
        self
    }

    fn combine_with(&mut self, other: Self);
}

impl<T> Combine for Option<T>
where
    T: Combine,
{
    fn combine(self, other: Self) -> Self
    where
        Self: Sized,
    {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (None, Some(b)) => Some(b),
            (a, _) => a,
        }
    }

    fn combine_with(&mut self, other: Self) {
        match (self, other) {
            (Some(a), Some(b)) => {
                a.combine_with(b);
            }
            (a @ None, Some(b)) => {
                *a = Some(b);
            }
            _ => {}
        }
    }
}

impl<T> Combine for Vec<T> {
    fn combine_with(&mut self, _other: Self) {
        // No-op, use own elements
    }
}

/// Implements [`Combine`] for a value that always returns `self` when combined with another value.
macro_rules! impl_noop_combine {
    ($name:ident) => {
        impl Combine for $name {
            #[inline(always)]
            fn combine_with(&mut self, _other: Self) {}

            #[inline(always)]
            fn combine(self, _other: Self) -> Self {
                self
            }
        }
    };
}

// std types
impl_noop_combine!(bool);
impl_noop_combine!(usize);
impl_noop_combine!(u8);
impl_noop_combine!(u16);
impl_noop_combine!(u32);
impl_noop_combine!(u64);
impl_noop_combine!(u128);
impl_noop_combine!(isize);
impl_noop_combine!(i8);
impl_noop_combine!(i16);
impl_noop_combine!(i32);
impl_noop_combine!(i64);
impl_noop_combine!(i128);
impl_noop_combine!(String);
