use super::options::DiagnosticMode;

use crate::session::options::ActiveEnvironment;
use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use ty_project::ProjectMetadata;
use ty_project::metadata::Options;
use ty_project::metadata::options::EnvironmentOptions;
use ty_project::metadata::value::{RangedValue, RelativePathBuf};

/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ClientSettings {
    pub(super) disable_language_services: bool,
    pub(super) diagnostic_mode: DiagnosticMode,
    pub(super) active_python_environment: Option<ActiveEnvironment>,
}

impl ClientSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }

    pub(crate) fn diagnostic_mode(&self) -> DiagnosticMode {
        self.diagnostic_mode
    }

    pub(crate) fn cli_overrides(&self, project: &ProjectMetadata) -> Options {
        let Some(active_environment) = &self.active_python_environment else {
            return Options::default();
        };

        let configured_python = project
            .options()
            .environment
            .as_ref()
            .and_then(|environment| environment.python.as_ref());
        let configured_python_version = project
            .options()
            .environment
            .as_ref()
            .and_then(|environment| environment.python_version.as_ref());

        let python = if configured_python.is_none() {
            if let Some(environment) = &active_environment.environment {
                environment.folder_uri.to_file_path().ok().and_then(|path| {
                    Some(RelativePathBuf::python_extension(
                        SystemPathBuf::from_path_buf(path).ok()?,
                    ))
                })
            } else {
                Some(RelativePathBuf::python_extension(
                    active_environment.executable.sys_prefix.clone(),
                ))
            }
        } else {
            None
        };

        if let Some(python) = &python {
            tracing::debug!(
                "Using the Python interpreter selected in the VS Code Python extension: {python}"
            );
        }

        let python_version = if configured_python_version.is_none() {
            active_environment.version.as_ref().and_then(|version| {
                Some(RangedValue::python_extension(PythonVersion::from((
                    u8::try_from(version.major).ok()?,
                    u8::try_from(version.minor).ok()?,
                ))))
            })
        } else {
            None
        };

        if let Some(python_version) = &python_version {
            tracing::debug!(
                "Using the Python version of the selected Python interpreter in the VS Code Python extension: {python_version}"
            );
        }

        Options {
            environment: Some(EnvironmentOptions {
                python,
                python_version,
                ..EnvironmentOptions::default()
            }),
            ..Options::default()
        }
    }
}
